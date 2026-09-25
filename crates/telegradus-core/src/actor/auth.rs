//! Login flow: TDLib authorization states, credentials and auth commands.

use std::future::Future;
use std::sync::Arc;

use tdlib_rs::enums::{self, AuthorizationState, EmailAddressAuthentication, ResendCodeReason};
use tdlib_rs::{functions, types};
use tracing::{info, warn};

use super::{Actor, Done, Phase};
use crate::Event;
use crate::config::{self, ApiCredentials};
use crate::convert;
use crate::errors::is_api_credentials_error;
use crate::model::{AuthState, ChatListId, ErrorContext};

/// `device_model` reported to Telegram (shown in the list of active sessions).
const DEVICE_MODEL: &str = "Desktop";

impl Actor {
    pub(super) fn on_authorization_state(&mut self, state: AuthorizationState) {
        self.phase = match &state {
            AuthorizationState::WaitTdlibParameters => Phase::WaitParameters,
            AuthorizationState::WaitCode(_) => Phase::WaitCode,
            AuthorizationState::WaitEmailCode(_) => Phase::WaitEmailCode,
            AuthorizationState::Ready => Phase::Ready,
            AuthorizationState::LoggingOut
            | AuthorizationState::Closing
            | AuthorizationState::Closed => Phase::Closing,
            _ => Phase::LoggingIn,
        };
        if let Some(auth) = convert::auth_state(&state) {
            self.emit(Event::Auth(auth));
        }
        match state {
            AuthorizationState::WaitTdlibParameters => self.provide_parameters(),
            AuthorizationState::WaitPremiumPurchase(_) => self.emit_local_error(
                ErrorContext::Auth,
                "Для входа с этим номером требуется Telegram Premium. Войдите через мобильное приложение или используйте другой номер.",
            ),
            AuthorizationState::Ready => self.on_ready(),
            AuthorizationState::Closed => self.on_closed(),
            _ => {}
        }
    }

    fn provide_parameters(&mut self) {
        match self.config.api.clone() {
            Some(api) => self.send_parameters(api),
            None => self.emit(Event::Auth(AuthState::NeedApiCredentials)),
        }
    }

    fn send_parameters(&mut self, api: ApiCredentials) {
        let data_dir = &self.config.data_dir;
        let database_directory = data_dir.join("tdlib").to_string_lossy().into_owned();
        let files_directory = data_dir.join("files").to_string_lossy().into_owned();
        let use_test_dc = self.config.use_test_dc;
        let language = convert::language_code(std::env::var("LANG").ok().as_deref());
        let client_id = self.client_id;
        // Waiting for TDLib's answer; a repeated `SetApiCredentials` only stores the values.
        self.phase = Phase::Starting;
        self.spawn(async move {
            let result = functions::set_tdlib_parameters(
                use_test_dc,
                database_directory,
                files_directory,
                String::new(),
                true,
                true,
                true,
                false,
                api.api_id,
                api.api_hash,
                language,
                DEVICE_MODEL.to_owned(),
                convert::os_name().to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
                client_id,
            )
            .await;
            Done::TdlibParameters(result)
        });
    }

    pub(super) fn on_parameters_set(&mut self, result: Result<(), types::Error>) {
        if let Err(err) = result {
            // TDLib still waits for parameters.
            self.phase = Phase::WaitParameters;
            self.report_auth_error(&err);
        }
    }

    pub(super) fn set_api_credentials(&mut self, api_id: i32, api_hash: &str) {
        let Some(credentials) = config::validate_credentials(api_id, api_hash) else {
            self.emit_local_error(
                ErrorContext::Auth,
                "Неверные api_id или api_hash: api_id — положительное число, api_hash — 32 шестнадцатеричных символа.",
            );
            return;
        };
        if let Err(err) = config::save_credentials(&self.config.data_dir, &credentials) {
            warn!("cannot save credentials: {err}");
            self.emit_local_error(
                ErrorContext::Other,
                format!("Не удалось сохранить настройки: {err}"),
            );
        }
        self.config.api = Some(credentials.clone());
        match self.phase {
            Phase::WaitParameters => {
                self.emit(Event::Auth(AuthState::Initializing));
                self.send_parameters(credentials);
            }
            // TDLib already runs with other credentials (which the server
            // rejected): restart it so the new ones are used.
            Phase::WaitCode | Phase::WaitEmailCode | Phase::LoggingIn => self.restart_client(),
            // Stored for the next start.
            Phase::Starting | Phase::Ready | Phase::Closing => {}
        }
    }

    /// Closes the current TDLib instance; [`Self::on_closed`] starts a new one.
    fn restart_client(&mut self) {
        info!("restarting TDLib with new credentials");
        self.phase = Phase::Closing;
        let client_id = self.client_id;
        self.spawn(
            async move { Done::Unit(ErrorContext::Other, functions::close(client_id).await) },
        );
    }

    fn on_closed(&mut self) {
        if self.shutting_down {
            self.finish();
        } else {
            // After logging out (or a restart) the user lands on the login screen again.
            self.reset_session();
            self.boot_client();
        }
    }

    /// Forgets everything about the previous account and tells the UI.
    fn reset_session(&mut self) {
        self.flush();
        self.store.clear();
        self.lists.clear();
        self.loading_lists.clear();
        self.open_chats.clear();
        let lists: Vec<ChatListId> = self.sent_lists.drain().map(|(list, _)| list).collect();
        for list in lists {
            self.emit(Event::ChatListUpdated {
                list,
                entries: Arc::from([]),
                has_more: true,
            });
        }
        self.emit(Event::FoldersChanged(Arc::from([])));
    }

    fn on_ready(&mut self) {
        let client_id = self.client_id;
        self.spawn(async move {
            let result = functions::get_me(client_id).await;
            Done::Me(result.map(|enums::User::User(user)| Box::new(user)))
        });
        self.load_chats(ChatListId::Main);
    }

    pub(super) fn on_me(&mut self, result: Result<Box<types::User>, types::Error>) {
        match result {
            Ok(user) => {
                self.store.set_my_id(user.id);
                let affected = self.store.upsert_user(&user);
                self.dirty.chats.extend(affected);
                self.emit(Event::Me(Arc::new(convert::me(&user))));
            }
            Err(err) => self.emit_error(ErrorContext::Other, &err),
        }
    }

    /// `updateUser` for the logged-in user refreshes [`Event::Me`].
    pub(super) fn on_user(&mut self, user: &types::User) {
        let affected = self.store.upsert_user(user);
        self.dirty.chats.extend(affected);
        if self.store.my_id() == Some(user.id) {
            self.emit(Event::Me(Arc::new(convert::me(user))));
        }
    }

    fn auth_request(
        &mut self,
        request: impl Future<Output = Result<(), types::Error>> + Send + 'static,
    ) {
        self.spawn(async move { Done::Auth(request.await) });
    }

    pub(super) fn on_auth_result(&mut self, result: Result<(), types::Error>) {
        if let Err(err) = result {
            self.report_auth_error(&err);
        }
    }

    /// Reports a failed login request. Rejected credentials send the UI back to
    /// the setup form first, so the error that follows is shown on that form.
    fn report_auth_error(&self, err: &types::Error) {
        if is_api_credentials_error(&err.message) {
            self.emit(Event::Auth(AuthState::NeedApiCredentials));
        }
        self.emit_error(ErrorContext::Auth, err);
    }

    pub(super) fn submit_phone_number(&mut self, phone: String) {
        let phone = phone.trim().to_owned();
        let client_id = self.client_id;
        self.auth_request(functions::set_authentication_phone_number(
            phone, None, client_id,
        ));
    }

    pub(super) fn request_qr_code(&mut self) {
        let client_id = self.client_id;
        self.auth_request(functions::request_qr_code_authentication(
            Vec::new(),
            client_id,
        ));
    }

    pub(super) fn submit_code(&mut self, code: String) {
        let code = code.trim().to_owned();
        let client_id = self.client_id;
        if self.phase == Phase::WaitEmailCode {
            let code =
                EmailAddressAuthentication::Code(types::EmailAddressAuthenticationCode { code });
            self.auth_request(functions::check_authentication_email_code(code, client_id));
        } else {
            self.auth_request(functions::check_authentication_code(code, client_id));
        }
    }

    pub(super) fn resend_code(&mut self) {
        let client_id = self.client_id;
        self.auth_request(functions::resend_authentication_code(
            Some(ResendCodeReason::UserRequest),
            client_id,
        ));
    }

    pub(super) fn submit_password(&mut self, password: String) {
        let client_id = self.client_id;
        self.auth_request(functions::check_authentication_password(
            password, client_id,
        ));
    }

    pub(super) fn log_out(&mut self) {
        let client_id = self.client_id;
        self.auth_request(functions::log_out(client_id));
    }
}
