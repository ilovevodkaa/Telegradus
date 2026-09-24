//! Small TDLib-to-model conversions: auth state, connection, files, chat lists, users.

use std::path::PathBuf;

use tdlib_rs::{
    enums::{self, AuthenticationCodeType, AuthorizationState},
    types,
};

use crate::chat_list::Position;
use crate::model::{
    AuthState, ChatKind, ChatListId, CodeInfo, CodeKind, ConnectionState, FileRef, Me,
};

/// Maps a TDLib authorization state. `WaitTdlibParameters` has no model
/// equivalent (the core answers it itself) and yields `None`.
pub(crate) fn auth_state(state: &AuthorizationState) -> Option<AuthState> {
    Some(match state {
        AuthorizationState::WaitTdlibParameters => return None,
        // Premium-only sign-in cannot be completed here; another number can still be entered.
        AuthorizationState::WaitPhoneNumber | AuthorizationState::WaitPremiumPurchase(_) => {
            AuthState::WaitPhoneNumber
        }
        AuthorizationState::WaitEmailAddress(_) => AuthState::WaitEmail,
        AuthorizationState::WaitEmailCode(wait) => AuthState::WaitCode(CodeInfo {
            phone_number: wait.code_info.email_address_pattern.clone(),
            kind: CodeKind::Email,
            length: wait.code_info.length,
            can_resend: true,
            timeout_secs: 0,
        }),
        AuthorizationState::WaitCode(wait) => AuthState::WaitCode(code_info(&wait.code_info)),
        AuthorizationState::WaitOtherDeviceConfirmation(wait) => AuthState::WaitQrConfirmation {
            link: wait.link.clone(),
        },
        AuthorizationState::WaitRegistration(_) => AuthState::WaitRegistration,
        AuthorizationState::WaitPassword(wait) => AuthState::WaitPassword {
            hint: wait.password_hint.clone(),
            has_recovery_email: wait.has_recovery_email_address,
        },
        AuthorizationState::Ready => AuthState::Ready,
        AuthorizationState::LoggingOut => AuthState::LoggingOut,
        AuthorizationState::Closing => AuthState::Closing,
        AuthorizationState::Closed => AuthState::Closed,
    })
}

fn code_info(info: &types::AuthenticationCodeInfo) -> CodeInfo {
    let (kind, length) = code_kind(&info.r#type);
    CodeInfo {
        phone_number: info.phone_number.clone(),
        kind,
        length,
        can_resend: info.next_type.is_some(),
        timeout_secs: info.timeout.max(0),
    }
}

fn code_kind(kind: &AuthenticationCodeType) -> (CodeKind, i32) {
    match kind {
        AuthenticationCodeType::TelegramMessage(t) => (CodeKind::TelegramMessage, t.length),
        AuthenticationCodeType::Sms(t) => (CodeKind::Sms, t.length),
        AuthenticationCodeType::SmsWord(_) | AuthenticationCodeType::SmsPhrase(_) => {
            (CodeKind::Sms, 0)
        }
        AuthenticationCodeType::Call(t) => (CodeKind::Call, t.length),
        AuthenticationCodeType::FlashCall(_) => (CodeKind::FlashCall, 0),
        AuthenticationCodeType::MissedCall(t) => (CodeKind::MissedCall, t.length),
        AuthenticationCodeType::Fragment(t) => (CodeKind::Fragment, t.length),
        AuthenticationCodeType::FirebaseAndroid(t) => (CodeKind::Sms, t.length),
        AuthenticationCodeType::FirebaseIos(t) => (CodeKind::Sms, t.length),
    }
}

pub(crate) fn connection_state(state: &enums::ConnectionState) -> ConnectionState {
    match state {
        enums::ConnectionState::WaitingForNetwork => ConnectionState::WaitingForNetwork,
        enums::ConnectionState::ConnectingToProxy => ConnectionState::ConnectingToProxy,
        enums::ConnectionState::Connecting => ConnectionState::Connecting,
        enums::ConnectionState::Updating => ConnectionState::Updating,
        enums::ConnectionState::Ready => ConnectionState::Ready,
    }
}

/// Maps a TDLib file; `local_path` is set only for completed downloads.
pub(crate) fn file_ref(file: &types::File) -> FileRef {
    let local = &file.local;
    let local_path = (local.is_downloading_completed && !local.path.is_empty())
        .then(|| PathBuf::from(&local.path));
    FileRef {
        id: file.id,
        size: if file.size > 0 {
            file.size
        } else {
            file.expected_size.max(0)
        },
        local_path,
        is_downloading: local.is_downloading_active,
        downloaded_size: local.downloaded_size,
    }
}

pub(crate) fn chat_list_id(list: &enums::ChatList) -> ChatListId {
    match list {
        enums::ChatList::Main => ChatListId::Main,
        enums::ChatList::Archive => ChatListId::Archive,
        enums::ChatList::Folder(folder) => ChatListId::Folder(folder.chat_folder_id),
    }
}

pub(crate) fn tdlib_chat_list(list: ChatListId) -> enums::ChatList {
    match list {
        ChatListId::Main => enums::ChatList::Main,
        ChatListId::Archive => enums::ChatList::Archive,
        ChatListId::Folder(chat_folder_id) => {
            enums::ChatList::Folder(types::ChatListFolder { chat_folder_id })
        }
    }
}

pub(crate) fn position(position: &types::ChatPosition) -> Position {
    Position {
        list: chat_list_id(&position.list),
        order: position.order,
        is_pinned: position.is_pinned,
    }
}

pub(crate) fn positions(positions: &[types::ChatPosition]) -> Vec<Position> {
    positions.iter().map(position).collect()
}

pub(crate) fn chat_kind(kind: &enums::ChatType) -> ChatKind {
    match kind {
        enums::ChatType::Private(p) => ChatKind::Private { user_id: p.user_id },
        enums::ChatType::Secret(s) => ChatKind::Secret { user_id: s.user_id },
        enums::ChatType::BasicGroup(_) => ChatKind::BasicGroup,
        enums::ChatType::Supergroup(s) if s.is_channel => ChatKind::Channel,
        enums::ChatType::Supergroup(_) => ChatKind::Supergroup,
    }
}

pub(crate) fn me(user: &types::User) -> Me {
    Me {
        id: user.id,
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        username: user
            .usernames
            .as_ref()
            .and_then(|u| u.active_usernames.first().cloned()),
        phone_number: user.phone_number.clone(),
    }
}

/// `"ru"` from `"ru_RU.UTF-8"`; `"en"` when unknown.
pub(crate) fn language_code(lang: Option<&str>) -> String {
    let code = lang
        .and_then(|l| l.split(['_', '.', '@', '-']).next())
        .map(str::to_ascii_lowercase)
        .filter(|c| (2..=3).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_lowercase()));
    code.unwrap_or_else(|| "en".to_owned())
}

/// Human-readable OS name for `system_version`, e.g. "Linux" or "Windows".
pub(crate) fn os_name() -> &'static str {
    match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        "freebsd" => "FreeBSD",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(completed: bool, path: &str) -> types::File {
        types::File {
            id: 7,
            size: 0,
            expected_size: 1000,
            local: types::LocalFile {
                path: path.into(),
                can_be_downloaded: true,
                can_be_deleted: false,
                is_downloading_active: !completed,
                is_downloading_completed: completed,
                download_offset: 0,
                downloaded_prefix_size: 0,
                downloaded_size: if completed { 1000 } else { 10 },
            },
            remote: types::RemoteFile {
                id: String::new(),
                unique_id: String::new(),
                is_uploading_active: false,
                is_uploading_completed: true,
                uploaded_size: 0,
            },
        }
    }

    #[test]
    fn file_path_only_when_completed() {
        let partial = file_ref(&file(false, "/tmp/partial"));
        assert_eq!(partial.local_path, None);
        assert!(partial.is_downloading);
        assert_eq!(partial.size, 1000);

        let done = file_ref(&file(true, "/tmp/done.jpg"));
        assert_eq!(done.local_path, Some(PathBuf::from("/tmp/done.jpg")));
        assert!(done.is_downloaded());
    }

    #[test]
    fn code_info_mapping() {
        let info = types::AuthenticationCodeInfo {
            phone_number: "+7900".into(),
            r#type: AuthenticationCodeType::Sms(types::AuthenticationCodeTypeSms { length: 5 }),
            next_type: Some(AuthenticationCodeType::Call(
                types::AuthenticationCodeTypeCall { length: 5 },
            )),
            timeout: 60,
        };
        let state =
            AuthorizationState::WaitCode(types::AuthorizationStateWaitCode { code_info: info });
        assert_eq!(
            auth_state(&state),
            Some(AuthState::WaitCode(CodeInfo {
                phone_number: "+7900".into(),
                kind: CodeKind::Sms,
                length: 5,
                can_resend: true,
                timeout_secs: 60,
            }))
        );
        assert_eq!(auth_state(&AuthorizationState::WaitTdlibParameters), None);
        assert_eq!(
            auth_state(&AuthorizationState::WaitPhoneNumber),
            Some(AuthState::WaitPhoneNumber)
        );
    }

    #[test]
    fn language_codes() {
        assert_eq!(language_code(Some("ru_RU.UTF-8")), "ru");
        assert_eq!(language_code(Some("de")), "de");
        assert_eq!(language_code(Some("C")), "en");
        assert_eq!(language_code(Some("C.UTF-8")), "en");
        assert_eq!(language_code(Some("")), "en");
        assert_eq!(language_code(None), "en");
    }
}
