use crate::model::{ChatId, ChatListId, FileId, MessageId};

/// Everything the UI can ask the core to do.
#[derive(Debug, Clone)]
pub enum Command {
    /// Store `api_id`/`api_hash` in the settings file and start TDLib with them.
    SetApiCredentials { api_id: i32, api_hash: String },

    SubmitPhoneNumber(String),
    /// Switch to QR-code login (from the phone-number step).
    RequestQrCode,
    SubmitCode(String),
    ResendCode,
    SubmitPassword(String),
    LogOut,

    /// Load more chats into a list. The core loads the first page of the main
    /// list by itself once logged in.
    LoadChats(ChatListId),

    /// Start showing a chat: the core opens it in TDLib and replies with an
    /// initial [`crate::Event::History`] page.
    OpenChat(ChatId),
    CloseChat(ChatId),
    /// Load the page of history before `from_message_id`.
    LoadOlder { chat_id: ChatId, from_message_id: MessageId },
    /// Mark messages as viewed (and read) by the user.
    ViewMessages { chat_id: ChatId, message_ids: Vec<MessageId> },

    SendText {
        chat_id: ChatId,
        text: String,
        reply_to: Option<MessageId>,
    },

    /// Start (or continue) downloading a file. Progress arrives as
    /// [`crate::Event::FileUpdated`]. Downloading an already downloaded file is a no-op.
    DownloadFile(FileId),

    /// Close TDLib cleanly. The core replies with [`crate::Event::Closed`].
    Shutdown,
}
