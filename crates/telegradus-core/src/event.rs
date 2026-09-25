use std::sync::Arc;

use crate::model::{
    AuthState, ChatId, ChatListEntry, ChatListId, ChatSummary, ConnectionState, CoreError, FileRef,
    Folder, Me, Message, MessageId,
};

/// Everything the core tells the UI.
///
/// Events are batched: after a burst of TDLib updates the core sends one
/// [`Event::ChatsUpdated`] with every changed chat and one
/// [`Event::ChatListUpdated`] per changed list, instead of one event per update.
#[derive(Debug, Clone)]
pub enum Event {
    Auth(AuthState),
    Connection(ConnectionState),
    Me(Arc<Me>),

    /// Chats whose summary changed (title, last message, unread count, photo, ...).
    ChatsUpdated(Vec<Arc<ChatSummary>>),
    /// The full, ordered contents of a chat list after it changed.
    ChatListUpdated {
        list: ChatListId,
        entries: Arc<[ChatListEntry]>,
        /// `false` once TDLib reported that every chat of this list is loaded.
        has_more: bool,
    },
    /// The user's chat folders, in display order.
    FoldersChanged(Arc<[Folder]>),

    /// A page of history for a chat opened with [`crate::Command::OpenChat`] or
    /// extended with [`crate::Command::LoadOlder`]. Messages are ordered from
    /// oldest to newest.
    History {
        chat_id: ChatId,
        messages: Vec<Message>,
        /// `true` for the first page after `OpenChat`, which replaces anything shown.
        is_initial: bool,
        /// `false` when the beginning of the chat was reached.
        has_more_older: bool,
    },
    /// A new message arrived (or was sent) in an open chat.
    MessageAdded(Message),
    /// An existing message changed (edited, content updated, sending failed).
    MessageUpdated(Message),
    /// A pending outgoing message was accepted by the server and got its final id.
    MessageSent {
        old_id: MessageId,
        message: Message,
    },
    MessagesDeleted {
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
    },

    /// Download progress or completion of a file.
    FileUpdated(FileRef),

    Error(CoreError),

    /// TDLib was closed after [`crate::Command::Shutdown`]; it is now safe to exit.
    Closed,
}
