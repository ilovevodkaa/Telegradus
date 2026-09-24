//! Readable Russian texts for well-known TDLib errors.

use crate::model::{CoreError, ErrorContext};
use crate::ru;

/// Builds a user-facing error from a TDLib error.
pub(crate) fn core_error(context: ErrorContext, error: &tdlib_rs::types::Error) -> CoreError {
    CoreError {
        context,
        code: error.code,
        message: describe(error.code, &error.message),
    }
}

/// A local (non-TDLib) error with code 0.
pub(crate) fn local_error(context: ErrorContext, message: impl Into<String>) -> CoreError {
    CoreError {
        context,
        code: 0,
        message: message.into(),
    }
}

/// Whether TDLib rejected the application's `api_id`/`api_hash`.
pub(crate) fn is_api_credentials_error(message: &str) -> bool {
    message.starts_with("API_ID_") || message.contains("api_id") || message.contains("api_hash")
}

/// Maps a TDLib error to readable Russian text; unknown errors keep the raw message.
pub(crate) fn describe(code: i32, message: &str) -> String {
    if let Some(seconds) = flood_wait_seconds(code, message) {
        return format!(
            "Слишком много попыток. Повторите через {}.",
            ru::wait_time(seconds)
        );
    }
    let known = match message {
        "PHONE_NUMBER_INVALID" | "Invalid phone number" => {
            "Неверный номер телефона. Укажите его в международном формате, например +7 900 123-45-67."
        }
        "PHONE_NUMBER_BANNED" => "Этот номер телефона заблокирован в Telegram.",
        "PHONE_NUMBER_FLOOD" => {
            "С этим номером было слишком много попыток входа. Попробуйте позже."
        }
        "PHONE_NUMBER_UNOCCUPIED" => "Этот номер не зарегистрирован в Telegram.",
        "PHONE_CODE_INVALID" | "CODE_INVALID" | "EMAIL_CODE_INVALID" => {
            "Неверный код. Попробуйте ещё раз."
        }
        "PHONE_CODE_EXPIRED" | "CODE_EXPIRED" | "EMAIL_CODE_EXPIRED" => {
            "Срок действия кода истёк. Запросите новый код."
        }
        "PHONE_CODE_EMPTY" | "CODE_EMPTY" => "Введите код подтверждения.",
        "PASSWORD_HASH_INVALID" => "Неверный пароль.",
        "PASSWORD_EMPTY" => "Введите пароль.",
        "SESSION_PASSWORD_NEEDED" => "Требуется пароль двухэтапной проверки.",
        "SEND_CODE_UNAVAILABLE" => "Не удалось отправить код повторно. Попробуйте войти позже.",
        "API_ID_INVALID" => "Неверные api_id или api_hash. Проверьте их на my.telegram.org.",
        "API_ID_PUBLISHED_FLOOD" => {
            "Эти api_id и api_hash опубликованы и заблокированы. Получите собственные на my.telegram.org."
        }
        "AUTH_KEY_UNREGISTERED" | "SESSION_REVOKED" | "SESSION_EXPIRED" | "USER_DEACTIVATED" => {
            "Сессия завершена. Войдите снова."
        }
        "USER_DEACTIVATED_BAN" => "Аккаунт заблокирован.",
        "PEER_FLOOD" => "Слишком много сообщений. Попробуйте позже.",
        "CHAT_WRITE_FORBIDDEN" | "Have no write access to the chat" => {
            "Нет прав на отправку сообщений в этот чат."
        }
        "USER_BANNED_IN_CHANNEL" => "Вам запрещено отправлять сообщения в группы и каналы.",
        "CHAT_RESTRICTED" | "CHAT_SEND_PLAIN_FORBIDDEN" => {
            "Отправка текстовых сообщений в этот чат ограничена."
        }
        "YOU_BLOCKED_USER" => "Вы заблокировали этого пользователя.",
        "USER_IS_BLOCKED" => "Пользователь ограничил отправку сообщений.",
        "MESSAGE_TOO_LONG" => "Сообщение слишком длинное.",
        "MESSAGE_EMPTY" | "Message must be non-empty" => "Сообщение не может быть пустым.",
        "Chat not found" => "Чат не найден.",
        "Message not found" => "Сообщение не найдено.",
        "File not found" | "Invalid file identifier" => "Файл не найден.",
        "Request aborted" => "Запрос прерван.",
        "Not Found" => "Не найдено.",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_owned();
    }
    if message.starts_with("Can't lock file") {
        return "База данных уже используется. Возможно, Telegradus уже запущен.".to_owned();
    }
    if message.starts_with("Valid api_id must be provided") || message.starts_with("API_ID_") {
        return "Неверные api_id или api_hash. Проверьте их на my.telegram.org.".to_owned();
    }
    if is_network_error(message) {
        return "Нет соединения с Telegram. Проверьте подключение к интернету.".to_owned();
    }
    if message.is_empty() {
        return format!("Ошибка Telegram (код {code}).");
    }
    message.to_owned()
}

fn is_network_error(message: &str) -> bool {
    const MARKERS: &[&str] = &[
        "NETWORK",
        "Network",
        "network",
        "Timeout",
        "TIMEOUT",
        "timed out",
        "Connection",
        "connection",
        "Failed to connect",
    ];
    MARKERS.iter().any(|marker| message.contains(marker))
}

/// Seconds to wait from `FLOOD_WAIT_N`-like errors or "Too Many Requests: retry after N".
fn flood_wait_seconds(code: i32, message: &str) -> Option<u64> {
    if let Some(rest) = message.strip_prefix("Too Many Requests: retry after ") {
        return rest.trim().parse().ok();
    }
    if message.contains("_WAIT_") && (message.contains("FLOOD") || code == 420) {
        let digits = message.rsplit('_').next()?;
        return digits.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_auth_errors_are_translated() {
        assert!(describe(400, "PHONE_NUMBER_INVALID").starts_with("Неверный номер"));
        assert_eq!(
            describe(400, "PHONE_CODE_INVALID"),
            "Неверный код. Попробуйте ещё раз."
        );
        assert!(describe(400, "PHONE_CODE_EXPIRED").contains("истёк"));
        assert_eq!(describe(400, "PASSWORD_HASH_INVALID"), "Неверный пароль.");
        assert!(describe(400, "PHONE_NUMBER_BANNED").contains("заблокирован"));
        assert!(describe(400, "API_ID_INVALID").contains("api_id"));
        assert!(
            describe(
                400,
                "Valid api_id must be provided. Can be obtained at https://my.telegram.org"
            )
            .contains("my.telegram.org")
        );
    }

    #[test]
    fn flood_wait_is_formatted() {
        assert_eq!(
            describe(420, "FLOOD_WAIT_30"),
            "Слишком много попыток. Повторите через 30 секунд."
        );
        assert_eq!(
            describe(429, "Too Many Requests: retry after 61"),
            "Слишком много попыток. Повторите через 2 минуты."
        );
        assert_eq!(
            describe(420, "FLOOD_WAIT_3600"),
            "Слишком много попыток. Повторите через 1 час."
        );
        assert_eq!(
            describe(420, "FLOOD_WAIT_3900"),
            "Слишком много попыток. Повторите через 1 час 5 минут."
        );
        assert_eq!(
            describe(420, "FLOOD_WAIT_1"),
            "Слишком много попыток. Повторите через 1 секунду."
        );
    }

    #[test]
    fn network_and_unknown_errors() {
        assert!(describe(500, "Connection closed").starts_with("Нет соединения"));
        assert!(describe(400, "Can't lock file \"td.binlog\"").contains("уже запущен"));
        assert_eq!(describe(400, "SOMETHING_ODD"), "SOMETHING_ODD");
        assert_eq!(describe(400, ""), "Ошибка Telegram (код 400).");
    }

    #[test]
    fn credentials_errors_are_detected() {
        assert!(is_api_credentials_error("API_ID_INVALID"));
        assert!(is_api_credentials_error("API_ID_PUBLISHED_FLOOD"));
        assert!(is_api_credentials_error("Valid api_id must be provided"));
        assert!(!is_api_credentials_error("PHONE_CODE_INVALID"));
    }
}
