//! Message history, message updates, sending and file downloads.

use tdlib_rs::enums::{self, InputMessageContent, InputMessageReplyTo};
use tdlib_rs::{functions, types};
use tracing::debug;

use super::{Actor, Done};
use crate::Event;
use crate::convert::file_ref;
use crate::history::{self, OpenChat, Page};
use crate::message::{self, Author};
use crate::model::{ChatId, ErrorContext, FileId, Message, MessageId};

/// Download priority for files the user is looking at (1-32).
const DOWNLOAD_PRIORITY: i32 = 16;

impl Actor {
    pub(super) fn open_chat(&mut self, chat_id: ChatId) {
        // Reopening reloads the history from scratch.
        let was_open = self
            .open_chats
            .insert(chat_id, OpenChat::default())
            .is_some();
        let client_id = self.client_id;
        self.spawn(async move {
            if !was_open && let Err(err) = functions::open_chat(chat_id, client_id).await {
                return Done::History {
                    chat_id,
                    initial: true,
                    result: Err(err),
                };
            }
            let result = history::fetch_page(client_id, chat_id, 0, history::PAGE_SIZE).await;
            Done::History {
                chat_id,
                initial: true,
                result,
            }
        });
    }

    pub(super) fn close_chat(&mut self, chat_id: ChatId) {
        if self.open_chats.remove(&chat_id).is_none() {
            return;
        }
        let client_id = self.client_id;
        self.spawn(async move {
            if let Err(err) = functions::close_chat(chat_id, client_id).await {
                debug!(chat_id, "closeChat failed: {}", err.message);
            }
            Done::Nothing
        });
    }

    pub(super) fn load_older(&mut self, chat_id: ChatId, from_message_id: MessageId) {
        let Some(chat) = self.open_chats.get_mut(&chat_id) else {
            return;
        };
        if std::mem::replace(&mut chat.loading_older, true) {
            return;
        }
        let client_id = self.client_id;
        self.spawn(async move {
            let result =
                history::fetch_page(client_id, chat_id, from_message_id, history::PAGE_SIZE).await;
            Done::History {
                chat_id,
                initial: false,
                result,
            }
        });
    }

    pub(super) fn on_history(
        &mut self,
        chat_id: ChatId,
        initial: bool,
        result: Result<Page, types::Error>,
    ) {
        if !self.open_chats.contains_key(&chat_id) {
            // Closed while loading.
            return;
        }
        let page = match result {
            Ok(page) => page,
            Err(err) => {
                if let Some(chat) = self.open_chats.get_mut(&chat_id)
                    && !initial
                {
                    chat.loading_older = false;
                }
                self.emit_error(ErrorContext::History, &err);
                return;
            }
        };
        let mut messages: Vec<_> = page
            .messages
            .into_iter()
            .map(|m| message::message(m, &self.store))
            .collect();
        messages.sort_by_key(|m| m.id);
        let Some(chat) = self.open_chats.get_mut(&chat_id) else {
            return;
        };
        if initial {
            for message in messages {
                chat.insert(message);
            }
            // Includes messages that arrived while the page was loading.
            let messages = chat.messages();
            self.emit(Event::History {
                chat_id,
                messages,
                is_initial: true,
                has_more_older: page.has_more,
            });
        } else {
            chat.loading_older = false;
            for message in &messages {
                chat.insert(message.clone());
            }
            self.emit(Event::History {
                chat_id,
                messages,
                is_initial: false,
                has_more_older: page.has_more,
            });
        }
    }

    pub(super) fn view_messages(&mut self, chat_id: ChatId, message_ids: Vec<MessageId>) {
        if message_ids.is_empty() {
            return;
        }
        let client_id = self.client_id;
        self.spawn(async move {
            if let Err(err) =
                functions::view_messages(chat_id, message_ids, None, true, client_id).await
            {
                debug!(chat_id, "viewMessages failed: {}", err.message);
            }
            Done::Nothing
        });
    }

    pub(super) fn send_text(&mut self, chat_id: ChatId, text: String, reply_to: Option<MessageId>) {
        if text.trim().is_empty() {
            return;
        }
        let reply_to = reply_to.map(|message_id| {
            InputMessageReplyTo::Message(types::InputMessageReplyToMessage {
                message_id,
                quote: None,
                checklist_task_id: 0,
            })
        });
        let content = InputMessageContent::InputMessageText(types::InputMessageText {
            text: types::FormattedText {
                text,
                entities: Vec::new(),
            },
            link_preview_options: None,
            clear_draft: true,
        });
        let client_id = self.client_id;
        self.spawn(async move {
            // The pending message itself arrives as `updateNewMessage`.
            let result =
                functions::send_message(chat_id, None, reply_to, None, content, client_id).await;
            Done::Unit(ErrorContext::Send, result.map(drop))
        });
    }

    pub(super) fn download_file(&mut self, file_id: FileId) {
        let client_id = self.client_id;
        self.spawn(async move {
            let result =
                functions::download_file(file_id, DOWNLOAD_PRIORITY, 0, 0, false, client_id).await;
            Done::File(result.map(|enums::File::File(file)| file))
        });
    }

    pub(super) fn on_file_result(&mut self, result: Result<types::File, types::Error>) {
        match result {
            Ok(file) => self.on_file(&file),
            Err(err) => self.emit_error(ErrorContext::File, &err),
        }
    }

    /// Records file progress; the latest state per file is sent on flush.
    pub(super) fn on_file(&mut self, file: &types::File) {
        let file = file_ref(file);
        if let Some(chat_id) = self.store.update_photo_file(&file) {
            self.dirty.chats.insert(chat_id);
        }
        self.dirty.files.insert(file.id, file);
    }

    pub(super) fn on_new_message(&mut self, message: types::Message) {
        if !self.open_chats.contains_key(&message.chat_id) {
            return;
        }
        let message = message::message(message, &self.store);
        if let Some(chat) = self.open_chats.get_mut(&message.chat_id) {
            chat.insert(message.clone());
            self.emit(Event::MessageAdded(message));
        }
    }

    pub(super) fn on_send_succeeded(&mut self, update: types::UpdateMessageSendSucceeded) {
        let chat_id = update.message.chat_id;
        let old_id = update.old_message_id;
        if self
            .store
            .rename_last_message(chat_id, old_id, update.message.id)
        {
            self.dirty.chats.insert(chat_id);
        }
        if !self.open_chats.contains_key(&chat_id) {
            return;
        }
        let message = message::message(update.message, &self.store);
        if let Some(chat) = self.open_chats.get_mut(&chat_id) {
            chat.remove(old_id);
            chat.insert(message.clone());
            self.emit(Event::MessageSent { old_id, message });
        }
    }

    pub(super) fn on_send_failed(&mut self, update: types::UpdateMessageSendFailed) {
        let chat_id = update.message.chat_id;
        let old_id = update.old_message_id;
        if self
            .store
            .rename_last_message(chat_id, old_id, update.message.id)
        {
            self.dirty.chats.insert(chat_id);
        }
        if !self.open_chats.contains_key(&chat_id) {
            return;
        }
        // The message carries `messageSendingStateFailed` with the error.
        let message = message::message(update.message, &self.store);
        let Some(chat) = self.open_chats.get_mut(&chat_id) else {
            return;
        };
        chat.remove(old_id);
        chat.insert(message.clone());
        if message.id == old_id {
            self.emit(Event::MessageUpdated(message));
        } else {
            // TDLib gives failed messages a new id: replace the pending one.
            self.emit(Event::MessagesDeleted {
                chat_id,
                message_ids: vec![old_id],
            });
            self.emit(Event::MessageAdded(message));
        }
    }

    pub(super) fn on_message_content(&mut self, update: types::UpdateMessageContent) {
        let types::UpdateMessageContent {
            chat_id,
            message_id,
            new_content,
        } = update;
        let open_chat = self.open_chats.get(&chat_id);
        let is_open = open_chat.is_some();
        // The cached message of an open chat, converted with its known sender.
        let content = open_chat
            .and_then(|chat| chat.get(message_id))
            .map(|cached| {
                let author = Author {
                    sender: &cached.sender,
                    name: &cached.sender_name,
                    chat_id,
                    is_outgoing: cached.is_outgoing,
                };
                message::content(new_content.clone(), &author, &self.store)
            });
        if self
            .store
            .update_last_message_content(chat_id, message_id, new_content)
        {
            self.dirty.chats.insert(chat_id);
        }
        match content {
            Some(content) => self.update_cached(chat_id, message_id, |m| m.content = content),
            None if is_open => self.refetch(chat_id, message_id),
            None => {}
        }
    }

    pub(super) fn on_message_edited(&mut self, update: types::UpdateMessageEdited) {
        let Some(chat) = self.open_chats.get(&update.chat_id) else {
            return;
        };
        if chat.get(update.message_id).is_none() {
            self.refetch(update.chat_id, update.message_id);
            return;
        }
        let edit_date = i64::from(update.edit_date);
        self.update_cached(update.chat_id, update.message_id, |m| {
            m.edit_date = edit_date;
        });
    }

    fn update_cached(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        change: impl FnOnce(&mut Message),
    ) {
        let Some(message) = self
            .open_chats
            .get_mut(&chat_id)
            .and_then(|chat| chat.get_mut(message_id))
        else {
            return;
        };
        change(message);
        let message = message.clone();
        self.emit(Event::MessageUpdated(message));
    }

    /// Loads a message missing from the cache so an update can be shown in full.
    fn refetch(&mut self, chat_id: ChatId, message_id: MessageId) {
        let client_id = self.client_id;
        self.spawn(async move {
            let result = functions::get_message(chat_id, message_id, client_id).await;
            Done::Refetched(result.map(|enums::Message::Message(message)| Box::new(message)))
        });
    }

    pub(super) fn on_refetched(&mut self, result: Result<Box<types::Message>, types::Error>) {
        let message = match result {
            Ok(message) => *message,
            Err(err) => {
                debug!("getMessage failed: {}", err.message);
                return;
            }
        };
        if !self.open_chats.contains_key(&message.chat_id) {
            return;
        }
        let message = message::message(message, &self.store);
        if let Some(chat) = self.open_chats.get_mut(&message.chat_id) {
            chat.insert(message.clone());
            self.emit(Event::MessageUpdated(message));
        }
    }

    pub(super) fn on_messages_deleted(&mut self, update: types::UpdateDeleteMessages) {
        // Messages merely evicted from TDLib's cache are not deletions.
        if !update.is_permanent {
            return;
        }
        let Some(chat) = self.open_chats.get_mut(&update.chat_id) else {
            return;
        };
        for id in &update.message_ids {
            chat.remove(*id);
        }
        self.emit(Event::MessagesDeleted {
            chat_id: update.chat_id,
            message_ids: update.message_ids,
        });
    }
}
