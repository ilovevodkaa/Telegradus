//! Plain data types shared between the core and the UI.
//!
//! These types are UI-agnostic snapshots of TDLib state. The UI never talks to
//! TDLib directly: it receives these models through [`crate::Event`]s.

use std::ops::Range;
use std::path::PathBuf;

pub type ChatId = i64;
pub type MessageId = i64;
pub type UserId = i64;
pub type FileId = i32;

/// Where the login flow currently is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    /// No `api_id`/`api_hash` configured yet; the UI must ask for them and
    /// send [`crate::Command::SetApiCredentials`].
    NeedApiCredentials,
    /// TDLib is starting up and has not reported a login state yet.
    Initializing,
    WaitPhoneNumber,
    /// QR login: show `link` as a QR code and wait for another device to confirm it.
    WaitQrConfirmation {
        link: String,
    },
    WaitCode(CodeInfo),
    WaitPassword {
        hint: String,
        has_recovery_email: bool,
    },
    /// The phone number is not registered. Sign-up is not supported by this client.
    WaitRegistration,
    /// Email login step. Not supported by the MVP.
    WaitEmail,
    Ready,
    LoggingOut,
    Closing,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeInfo {
    pub phone_number: String,
    pub kind: CodeKind,
    /// Expected code length, if known (0 when unknown).
    pub length: i32,
    /// Whether [`crate::Command::ResendCode`] can be used.
    pub can_resend: bool,
    /// Seconds before the code can be resent (0 when not limited).
    pub timeout_secs: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    /// Code sent as a message to another logged-in Telegram app.
    TelegramMessage,
    Sms,
    Call,
    FlashCall,
    MissedCall,
    Fragment,
    Email,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    WaitingForNetwork,
    ConnectingToProxy,
    Connecting,
    Updating,
    Ready,
}

/// The logged-in user.
#[derive(Debug, Clone, PartialEq)]
pub struct Me {
    pub id: UserId,
    pub first_name: String,
    pub last_name: String,
    pub username: Option<String>,
    pub phone_number: String,
}

/// A chat list: the main list, the archive or a user-defined folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChatListId {
    Main,
    Archive,
    Folder(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: i32,
    pub title: String,
}

/// One row of an ordered chat list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatListEntry {
    pub chat_id: ChatId,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatKind {
    Private { user_id: UserId },
    Secret { user_id: UserId },
    BasicGroup,
    Supergroup,
    Channel,
}

/// Everything the UI needs to draw a chat in the list and in the chat header.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatSummary {
    pub id: ChatId,
    pub title: String,
    pub kind: ChatKind,
    /// Small avatar. Download it with [`crate::Command::DownloadFile`] when shown.
    pub photo: Option<FileRef>,
    pub last_message: Option<MessagePreview>,
    pub unread_count: i32,
    pub unread_mention_count: i32,
    pub is_muted: bool,
    /// Text of the unsent draft, if any.
    pub draft: Option<String>,
    /// Whether the current user can send text messages here.
    pub can_send_messages: bool,
    /// Id of the newest message the user has read in this chat.
    pub last_read_inbox_message_id: MessageId,
}

/// Short, single-line description of a message for the chat list.
#[derive(Debug, Clone, PartialEq)]
pub struct MessagePreview {
    pub id: MessageId,
    /// Unix timestamp (seconds).
    pub date: i64,
    pub is_outgoing: bool,
    /// Sender name for group chats, `None` for private chats and channels.
    pub sender_name: Option<String>,
    /// Plain-text preview, e.g. the message text or "Photo".
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sender {
    User(UserId),
    Chat(ChatId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SendingState {
    Sent,
    Pending,
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub sender: Sender,
    /// Display name of the sender, resolved by the core when the message is emitted.
    pub sender_name: String,
    /// Unix timestamp (seconds).
    pub date: i64,
    /// Unix timestamp of the last edit, 0 if never edited.
    pub edit_date: i64,
    pub is_outgoing: bool,
    pub sending_state: SendingState,
    pub reply_to: Option<MessageId>,
    pub content: MessageContent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageContent {
    Text(FormattedText),
    Photo {
        /// The best size for inline display (not the original).
        file: FileRef,
        width: i32,
        height: i32,
        caption: FormattedText,
    },
    Document {
        file_name: String,
        file: FileRef,
        caption: FormattedText,
    },
    Video {
        duration: i32,
        caption: FormattedText,
    },
    Animation {
        caption: FormattedText,
    },
    Audio {
        title: String,
        performer: String,
        duration: i32,
    },
    Voice {
        duration: i32,
        caption: FormattedText,
    },
    VideoNote {
        duration: i32,
    },
    Sticker {
        emoji: String,
    },
    /// Service message, already rendered as human-readable text
    /// (for example "Alice joined the group").
    Service(String),
    /// Content the MVP does not render; the string is a short label such as "Poll".
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormattedText {
    pub text: String,
    /// Sorted by `range.start`. May nest (e.g. bold inside a link) but never
    /// partially overlap.
    pub entities: Vec<TextEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntity {
    /// Byte range into [`FormattedText::text`]; always on UTF-8 char boundaries.
    /// (The core converts TDLib's UTF-16 offsets.)
    pub range: Range<usize>,
    pub kind: TextEntityKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEntityKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Spoiler,
    Code,
    Pre {
        language: String,
    },
    Blockquote,
    /// A URL written literally in the text.
    Url,
    /// Text that links to `url`.
    TextUrl {
        url: String,
    },
    Email,
    Mention,
    MentionName {
        user_id: UserId,
    },
    Hashtag,
    BotCommand,
    PhoneNumber,
    Other,
}

/// A file known to TDLib and its local download state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    pub id: FileId,
    /// Size in bytes, 0 if unknown.
    pub size: i64,
    /// Path of the local copy once it is fully downloaded.
    pub local_path: Option<PathBuf>,
    pub is_downloading: bool,
    pub downloaded_size: i64,
}

impl FileRef {
    pub fn is_downloaded(&self) -> bool {
        self.local_path.is_some()
    }
}

/// A user-facing error, e.g. "PHONE_CODE_INVALID" mapped to readable text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    pub context: ErrorContext,
    /// Raw TDLib error code (400, 401, 420, ...), 0 for local errors.
    pub code: i32,
    pub message: String,
}

/// What the failed operation was, so the UI can show the error in the right place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorContext {
    Auth,
    ChatList,
    History,
    Send,
    File,
    Other,
}
