//! Setup and login screens.

use std::time::{Duration, Instant};

use iced::widget::{Space, button, column, container, operation, qr_code, row, text, text_input};
use iced::{Alignment, Element, Length, Padding, Task};
use telegradus_core::{AuthState, CodeInfo, CodeKind, Command, ConnectionState};

use crate::app::Message;
use crate::format;
use crate::theme::{self, MONO, SANS_MEDIUM, SANS_SEMIBOLD};
use crate::ui::widgets::{self, mono, wordmark};

const API_SITE: &str = "https://my.telegram.org/apps";
/// The first input of every login step, focused when the step appears.
pub const FIELD_ID: &str = "auth-field";
const SECOND_FIELD_ID: &str = "auth-field-2";

#[derive(Debug, Clone)]
pub enum Msg {
    ApiId(String),
    ApiHash(String),
    SubmitApi,
    OpenApiSite,
    Phone(String),
    SubmitPhone,
    UseQr,
    UsePhone,
    Code(String),
    SubmitCode,
    ResendCode,
    Password(String),
    SubmitPassword,
}

/// Contents and status of the login forms.
#[derive(Default)]
pub struct Form {
    api_id: String,
    api_hash: String,
    phone: String,
    code: String,
    password: String,
    error: Option<String>,
    /// A request is in flight: submit buttons are disabled.
    pending: bool,
    /// Show the phone form although the core waits for something else
    /// (switching away from QR login or changing the number).
    force_phone: bool,
    resend_at: Option<Instant>,
    qr: Option<(String, qr_code::Data)>,
}

impl Form {
    pub fn update(&mut self, msg: Msg, state: &AuthState) -> (Option<Command>, Task<Message>) {
        let mut command = None;
        let mut task = Task::none();
        match msg {
            Msg::ApiId(value) => {
                self.api_id = value
                    .chars()
                    .filter(char::is_ascii_digit)
                    .take(12)
                    .collect()
            }
            Msg::ApiHash(value) => self.api_hash = value.trim().to_owned(),
            Msg::SubmitApi => match self.api_id.parse::<i32>() {
                Ok(api_id) if api_id > 0 => {
                    let hash_ok = self.api_hash.len() == 32
                        && self.api_hash.chars().all(|c| c.is_ascii_hexdigit());
                    if hash_ok {
                        command = Some(self.submit(Command::SetApiCredentials {
                            api_id,
                            api_hash: self.api_hash.clone(),
                        }));
                    } else {
                        self.error =
                            Some("api_hash — это строка из 32 символов (0–9, a–f).".into());
                    }
                }
                _ => self.error = Some("api_id — это число, например 1234567.".into()),
            },
            Msg::OpenApiSite => task = Task::done(Message::OpenUrl(API_SITE.into())),
            Msg::Phone(value) => {
                self.phone = value
                    .chars()
                    .filter(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-' | '(' | ')'))
                    .take(24)
                    .collect();
            }
            Msg::SubmitPhone => {
                let digits: String = self.phone.chars().filter(char::is_ascii_digit).collect();
                if digits.len() < 7 {
                    self.error = Some("Введите номер полностью, с кодом страны.".into());
                } else {
                    command = Some(self.submit(Command::SubmitPhoneNumber(format!("+{digits}"))));
                }
            }
            Msg::UseQr => {
                self.force_phone = false;
                if matches!(state, AuthState::WaitQrConfirmation { .. }) {
                    return (None, Task::none());
                }
                command = Some(self.submit(Command::RequestQrCode));
            }
            Msg::UsePhone => {
                self.force_phone = true;
                self.error = None;
                task = operation::focus(FIELD_ID);
            }
            Msg::Code(value) => {
                self.code = value
                    .chars()
                    .filter(char::is_ascii_digit)
                    .take(12)
                    .collect();
                self.error = None;
                let expected = match state {
                    AuthState::WaitCode(info) => usize::try_from(info.length).unwrap_or(0),
                    _ => 0,
                };
                if expected > 0 && self.code.len() == expected && !self.pending {
                    command = Some(self.submit(Command::SubmitCode(self.code.clone())));
                }
            }
            Msg::SubmitCode => {
                if !self.code.is_empty() {
                    command = Some(self.submit(Command::SubmitCode(self.code.clone())));
                }
            }
            Msg::ResendCode => command = Some(self.submit(Command::ResendCode)),
            Msg::Password(value) => {
                self.password = value;
                self.error = None;
            }
            Msg::SubmitPassword => {
                if !self.password.is_empty() {
                    command = Some(self.submit(Command::SubmitPassword(self.password.clone())));
                }
            }
        }
        (command, task)
    }

    fn submit(&mut self, command: Command) -> Command {
        self.pending = true;
        self.error = None;
        command
    }

    /// The core moved to another login step. Returns a task focusing the
    /// step's input, if it has one.
    pub fn on_state_change(&mut self, state: &AuthState) -> Task<Message> {
        self.pending = false;
        self.error = None;
        self.force_phone = false;
        self.resend_at = None;
        self.qr = None;
        match state {
            AuthState::WaitCode(info) => {
                self.code.clear();
                if info.timeout_secs > 0 {
                    self.resend_at = Some(
                        Instant::now()
                            + Duration::from_secs(info.timeout_secs.unsigned_abs().into()),
                    );
                }
            }
            AuthState::WaitPassword { .. } => self.password.clear(),
            AuthState::WaitQrConfirmation { link } => match qr_code::Data::new(link) {
                Ok(data) => self.qr = Some((link.clone(), data)),
                Err(error) => tracing::warn!(%error, "cannot encode the login QR code"),
            },
            AuthState::Ready => {
                self.password.clear();
                self.code.clear();
            }
            _ => {}
        }
        match state {
            AuthState::NeedApiCredentials
            | AuthState::WaitPhoneNumber
            | AuthState::WaitCode(_)
            | AuthState::WaitPassword { .. } => operation::focus(FIELD_ID),
            _ => Task::none(),
        }
    }

    pub fn on_error(&mut self, message: String) {
        self.pending = false;
        self.error = Some(message);
    }

    /// Seconds until the code can be resent, while a countdown is shown.
    pub fn resend_countdown(&self, state: &AuthState) -> Option<u64> {
        if !matches!(state, AuthState::WaitCode(_)) || self.force_phone {
            return None;
        }
        let left = self.resend_at?.saturating_duration_since(Instant::now());
        (!left.is_zero()).then(|| left.as_secs() + u64::from(left.subsec_nanos() > 0))
    }
}

/// The login flow screen for the current state.
pub fn view<'a>(
    form: &'a Form,
    state: &'a AuthState,
    connection: ConnectionState,
) -> Element<'a, Message> {
    let card: Element<'a, Message> = if form.force_phone {
        phone(form)
    } else {
        match state {
            AuthState::NeedApiCredentials => setup(form),
            AuthState::WaitPhoneNumber => phone(form),
            AuthState::WaitQrConfirmation { .. } => qr(form),
            AuthState::WaitCode(info) => code(form, info, state),
            AuthState::WaitPassword { hint, .. } => password(form, hint),
            AuthState::WaitRegistration => unsupported(
                "Номер не зарегистрирован",
                "Регистрация новых аккаунтов в Telegradus пока не поддерживается. Создайте аккаунт в официальном приложении Telegram и возвращайтесь.",
            ),
            AuthState::WaitEmail => unsupported(
                "Вход по почте",
                "Telegram просит подтвердить вход через электронную почту. Этот способ пока не поддерживается — попробуйте войти по QR-коду.",
            ),
            AuthState::Initializing | AuthState::Ready => {
                loading("Подключаемся к Telegram…", form.error.as_deref())
            }
            AuthState::LoggingOut => loading("Выходим из аккаунта…", form.error.as_deref()),
            AuthState::Closing | AuthState::Closed => {
                loading("Завершаем работу…", form.error.as_deref())
            }
        }
    };
    // A request that hangs offline otherwise looks like a frozen form.
    let footer = if form.pending && connection != ConnectionState::Ready {
        widgets::connection(connection)
    } else {
        mono("свободный клиент Telegram · GPL-3.0", 11.0)
            .style(theme::text_muted)
            .into()
    };
    frame(card, footer)
}

/// Page background, wordmark, card and footer.
fn frame<'a>(card: Element<'a, Message>, footer: Element<'a, Message>) -> Element<'a, Message> {
    let header = container(wordmark(22.0)).padding(Padding::default().bottom(4));
    let content = column![
        header,
        container(card)
            .padding(36)
            .width(Length::Fill)
            .style(theme::card),
        footer,
    ]
    .spacing(18)
    .max_width(420)
    .align_x(Alignment::Center);
    container(content)
        .center(Length::Fill)
        .padding(24)
        .style(theme::page)
        .into()
}

fn step<'a>(label: &'a str) -> Element<'a, Message> {
    mono(label, 11.0).style(theme::text_muted).into()
}

fn title<'a>(content: &'a str) -> Element<'a, Message> {
    text(content).font(SANS_SEMIBOLD).size(24).into()
}

fn description<'a>(content: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(content)
        .size(14)
        .line_height(1.45)
        .style(theme::text_muted)
        .into()
}

fn label<'a>(content: &'a str) -> Element<'a, Message> {
    mono(content, 11.0).style(theme::text_muted).into()
}

fn field<'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Msg,
    on_submit: Msg,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |v| Message::Auth(on_input(v)))
        .on_submit(Message::Auth(on_submit))
        .padding(Padding::from([11.0, 14.0]))
        .size(15)
        .style(theme::input)
}

fn primary<'a>(label: &'a str, pending: bool, on_press: Option<Msg>) -> Element<'a, Message> {
    let label = if pending {
        "Подождите…"
    } else {
        label
    };
    button(container(text(label).font(SANS_SEMIBOLD).size(15)).center_x(Length::Fill))
        .width(Length::Fill)
        .padding(Padding::from([12.0, 20.0]))
        .style(theme::button_primary)
        .on_press_maybe(if pending {
            None
        } else {
            on_press.map(Message::Auth)
        })
        .into()
}

fn secondary<'a>(label: &'a str, on_press: Msg) -> Element<'a, Message> {
    button(container(text(label).font(SANS_MEDIUM).size(14)).center_x(Length::Fill))
        .width(Length::Fill)
        .padding(Padding::from([11.0, 20.0]))
        .style(theme::button_secondary)
        .on_press(Message::Auth(on_press))
        .into()
}

fn link<'a>(label: &'a str, on_press: Option<Msg>) -> Element<'a, Message> {
    button(text(label).font(SANS_MEDIUM).size(14))
        .padding(Padding::from([6.0, 10.0]))
        .style(theme::button_ghost)
        .on_press_maybe(on_press.map(Message::Auth))
        .into()
}

fn error<'a>(form: &'a Form) -> Option<Element<'a, Message>> {
    form.error
        .as_deref()
        .map(|message| text(message).size(13).style(theme::text_danger).into())
}

fn setup(form: &Form) -> Element<'_, Message> {
    let site = button(mono("my.telegram.org/apps ↗", 13.0))
        .padding(Padding::from([5.0, 10.0]))
        .style(theme::button_secondary)
        .on_press(Message::Auth(Msg::OpenApiSite));
    let ready = !form.api_id.is_empty() && !form.api_hash.is_empty();
    column![
        step("01 — ключи API"),
        title("Подключение к Telegram"),
        description(
            "Telegradus работает через официальный API Telegram. Создайте приложение на сайте Telegram — это займёт минуту — и вставьте сюда api_id и api_hash."
        ),
        site,
        Space::new().height(4),
        column![
            label("api_id"),
            field("1234567", &form.api_id, Msg::ApiId, Msg::SubmitApi).id(FIELD_ID),
        ]
        .spacing(6),
        column![
            label("api_hash"),
            field("0123456789abcdef0123456789abcdef", &form.api_hash, Msg::ApiHash, Msg::SubmitApi).id(SECOND_FIELD_ID),
        ]
        .spacing(6),
    ]
    .push(error(form))
    .push(primary("Продолжить", form.pending, ready.then_some(Msg::SubmitApi)))
    .push(description("Ключи хранятся только на этом компьютере."))
    .spacing(14)
    .into()
}

fn phone(form: &Form) -> Element<'_, Message> {
    let ready = form.phone.chars().filter(char::is_ascii_digit).count() >= 7;
    column![
        step("02 — вход"),
        title("Вход в Telegram"),
        description(
            "Введите номер телефона, к которому привязан аккаунт. Мы отправим код подтверждения."
        ),
        column![
            label("номер телефона"),
            field(
                "+7 900 000-00-00",
                &form.phone,
                Msg::Phone,
                Msg::SubmitPhone
            )
            .id(FIELD_ID),
        ]
        .spacing(6),
    ]
    .push(error(form))
    .push(primary(
        "Далее",
        form.pending,
        ready.then_some(Msg::SubmitPhone),
    ))
    .push(secondary("Войти по QR-коду", Msg::UseQr))
    .spacing(14)
    .into()
}

fn qr(form: &Form) -> Element<'_, Message> {
    let code: Element<'_, Message> = match &form.qr {
        Some((_, data)) => container(qr_code(data).cell_size(5).style(|_| qr_code::Style {
            cell: theme::LIGHT.text,
            background: theme::LIGHT.bg,
        }))
        .padding(12)
        .style(|_| container::Style {
            background: Some(theme::LIGHT.bg.into()),
            border: iced::border::rounded(16).width(1).color(theme::LIGHT.line),
            ..container::Style::default()
        })
        .into(),
        None => container(mono("QR-код недоступен", 12.0).style(theme::text_muted))
            .center(220)
            .style(theme::media_placeholder)
            .into(),
    };
    let steps = [
        "Откройте Telegram на телефоне",
        "Настройки → Устройства → Подключить устройство",
        "Наведите камеру на этот код",
    ];
    let steps = column(steps.into_iter().enumerate().map(|(i, step)| {
        row![
            container(mono(format!("{}", i + 1), 11.0))
                .center_x(22)
                .center_y(22)
                .style(theme::service_pill),
            text(step).size(14),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    }))
    .spacing(10);
    column![
        step("02 — вход по QR-коду"),
        title("Сканируйте код"),
        container(code).center_x(Length::Fill),
        steps,
    ]
    .push(error(form))
    .push(secondary("Войти по номеру телефона", Msg::UsePhone))
    .spacing(16)
    .into()
}

fn code_hint(info: &CodeInfo) -> String {
    // Keep the number on one line.
    let phone = info.phone_number.replace(' ', "\u{a0}");
    match info.kind {
        CodeKind::TelegramMessage => {
            "Мы отправили код в Telegram на другом вашем устройстве — он придёт в чат «Telegram»."
                .to_owned()
        }
        CodeKind::Sms => format!("Мы отправили SMS с кодом на номер {phone}."),
        CodeKind::Call => format!("Сейчас на номер {phone} позвонит робот и продиктует код."),
        CodeKind::FlashCall => format!(
            "На номер {phone} поступит звонок — сбрасывать не нужно, код придёт автоматически."
        ),
        CodeKind::MissedCall => format!(
            "На номер {phone} поступит звонок. Введите последние цифры номера, с которого звонили."
        ),
        CodeKind::Fragment => "Код отправлен в приложение Fragment.".to_owned(),
        CodeKind::Email => "Мы отправили код на вашу электронную почту.".to_owned(),
        CodeKind::Other => format!("Введите код, отправленный на номер {phone}."),
    }
}

fn code<'a>(form: &'a Form, info: &'a CodeInfo, state: &AuthState) -> Element<'a, Message> {
    let placeholder = match usize::try_from(info.length) {
        Ok(n @ 1..=12) => "•".repeat(n),
        _ => "Код".to_owned(),
    };
    let input = text_input(&placeholder, &form.code)
        .id(FIELD_ID)
        .on_input(|v| Message::Auth(Msg::Code(v)))
        .on_submit(Message::Auth(Msg::SubmitCode))
        .padding(Padding::from([12.0, 14.0]))
        .size(22)
        .font(MONO)
        .style(theme::input);
    let resend: Element<'a, Message> = match form.resend_countdown(state) {
        Some(left) => mono(
            format!(
                "Повторная отправка через {}",
                format::duration(left.min(i32::MAX as u64) as i32)
            ),
            12.0,
        )
        .style(theme::text_muted)
        .into(),
        None if info.can_resend => link(
            "Отправить код повторно",
            (!form.pending).then_some(Msg::ResendCode),
        ),
        None => Space::new().into(),
    };
    column![
        step("03 — подтверждение"),
        title("Код подтверждения"),
        description(code_hint(info)),
        input,
    ]
    .push(error(form))
    .push(primary(
        "Войти",
        form.pending,
        (!form.code.is_empty()).then_some(Msg::SubmitCode),
    ))
    .push(
        row![
            resend,
            Space::new().width(Length::Fill),
            link("Изменить номер", Some(Msg::UsePhone))
        ]
        .align_y(Alignment::Center),
    )
    .spacing(14)
    .into()
}

fn password<'a>(form: &'a Form, hint: &'a str) -> Element<'a, Message> {
    let placeholder = if hint.is_empty() {
        "Пароль"
    } else {
        hint
    };
    let input = text_input(placeholder, &form.password)
        .id(FIELD_ID)
        .secure(true)
        .on_input(|v| Message::Auth(Msg::Password(v)))
        .on_submit(Message::Auth(Msg::SubmitPassword))
        .padding(Padding::from([11.0, 14.0]))
        .size(15)
        .style(theme::input);
    column![
        step("04 — двухэтапная проверка"),
        title("Облачный пароль"),
        description("Аккаунт защищён облачным паролем. Введите его, чтобы завершить вход."),
        column![label("пароль"), input].spacing(6),
    ]
    .push(error(form))
    .push(primary(
        "Войти",
        form.pending,
        (!form.password.is_empty()).then_some(Msg::SubmitPassword),
    ))
    .spacing(14)
    .into()
}

fn unsupported<'a>(heading: &'a str, body: &'a str) -> Element<'a, Message> {
    column![
        step("вход"),
        title(heading),
        description(body),
        secondary("Ввести другой номер", Msg::UsePhone),
        secondary("Войти по QR-коду", Msg::UseQr),
    ]
    .spacing(14)
    .into()
}

/// A waiting card; `error` replaces the hint when the step failed (for
/// example, when another instance holds the database).
fn loading<'a>(label: &'a str, error: Option<&'a str>) -> Element<'a, Message> {
    // The dots promise progress, so they go away with an error.
    let (dots, hint): (Option<Element<'a, Message>>, Element<'a, Message>) = match error {
        Some(message) => (
            None,
            text(message)
                .size(13)
                .align_x(Alignment::Center)
                .style(theme::text_danger)
                .into(),
        ),
        None => (
            Some(mono("● ● ●", 12.0).style(theme::text_muted).into()),
            mono("это займёт пару секунд", 11.0)
                .style(theme::text_muted)
                .into(),
        ),
    };
    column![dots, text(label).size(16).font(SANS_MEDIUM), hint]
        .spacing(10)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .into()
}
