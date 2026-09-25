//! Chats, chat lists and folders.

use std::sync::Arc;

use tdlib_rs::enums::Update;
use tdlib_rs::{functions, types};

use super::{Actor, Done};
use crate::Event;
use crate::convert;
use crate::model::{ChatId, ChatListId, ErrorContext, Folder};

/// Chats requested per `loadChats` call.
const CHAT_PAGE: i32 = 100;
/// TDLib's answer to `loadChats` once every chat of the list is known.
const ALL_CHATS_LOADED: i32 = 404;

impl Actor {
    /// Handles updates about chats, users and groups; others are ignored.
    pub(super) fn on_chat_update(&mut self, update: Update) {
        match update {
            Update::NewChat(u) => {
                let chat_id = u.chat.id;
                let positions = convert::positions(&u.chat.positions);
                self.store.add_chat(u.chat);
                self.lists
                    .replace_positions(chat_id, &positions, &mut self.dirty.lists);
                self.dirty.chats.insert(chat_id);
            }
            Update::ChatTitle(u) => {
                self.store.set_title(u.chat_id, u.title);
                self.touch(u.chat_id);
            }
            Update::ChatPhoto(u) => {
                self.store.set_photo(u.chat_id, u.photo.as_ref());
                self.touch(u.chat_id);
            }
            Update::ChatPermissions(u) => {
                self.store.set_permissions(u.chat_id, &u.permissions);
                self.touch(u.chat_id);
            }
            Update::ChatLastMessage(u) => {
                self.store.set_last_message(u.chat_id, u.last_message);
                self.replace_positions(u.chat_id, &u.positions);
                self.touch(u.chat_id);
            }
            Update::ChatPosition(u) => {
                let position = convert::position(&u.position);
                if self.lists.set_position(u.chat_id, position) {
                    self.dirty.lists.insert(position.list);
                }
            }
            Update::ChatReadInbox(u) => {
                self.store
                    .set_read_inbox(u.chat_id, u.last_read_inbox_message_id, u.unread_count);
                self.touch(u.chat_id);
            }
            Update::ChatUnreadMentionCount(u) => {
                self.store
                    .set_unread_mention_count(u.chat_id, u.unread_mention_count);
                self.touch(u.chat_id);
            }
            Update::MessageMentionRead(u) => {
                self.store
                    .set_unread_mention_count(u.chat_id, u.unread_mention_count);
                self.touch(u.chat_id);
            }
            Update::ChatNotificationSettings(u) => {
                self.store
                    .set_notification_settings(u.chat_id, &u.notification_settings);
                self.touch(u.chat_id);
            }
            Update::ScopeNotificationSettings(u) => {
                let affected = self
                    .store
                    .set_scope_mute_for(&u.scope, u.notification_settings.mute_for);
                self.dirty.chats.extend(affected);
            }
            Update::ChatDraftMessage(u) => {
                self.store.set_draft(u.chat_id, u.draft_message.as_ref());
                self.replace_positions(u.chat_id, &u.positions);
                self.touch(u.chat_id);
            }
            Update::User(u) => self.on_user(&u.user),
            Update::BasicGroup(u) => {
                if let Some(chat_id) = self.store.upsert_basic_group(&u.basic_group) {
                    self.touch(chat_id);
                }
            }
            Update::Supergroup(u) => {
                if let Some(chat_id) = self.store.upsert_supergroup(&u.supergroup) {
                    self.touch(chat_id);
                }
            }
            Update::ChatFolders(u) => self.on_folders(&u.chat_folders),
            _ => {}
        }
    }

    /// Marks a chat's summary for the next flush.
    fn touch(&mut self, chat_id: ChatId) {
        if self.store.contains_chat(chat_id) {
            self.dirty.chats.insert(chat_id);
        }
    }

    fn replace_positions(&mut self, chat_id: ChatId, positions: &[types::ChatPosition]) {
        let positions = convert::positions(positions);
        self.lists
            .replace_positions(chat_id, &positions, &mut self.dirty.lists);
    }

    fn on_folders(&mut self, folders: &[types::ChatFolderInfo]) {
        let folders: Arc<[Folder]> = folders
            .iter()
            .map(|f| Folder {
                id: f.id,
                title: f.name.text.text.clone(),
            })
            .collect();
        self.emit(Event::FoldersChanged(folders));
    }

    /// Requests the next page of a list; the chats arrive as updates.
    pub(super) fn load_chats(&mut self, list: ChatListId) {
        if !self.lists.has_more(list) {
            // Nothing left to load: repeat the final state so the UI can stop waiting.
            self.dirty.forced_lists.insert(list);
            return;
        }
        if !self.loading_lists.insert(list) {
            return;
        }
        let client_id = self.client_id;
        let chat_list = convert::tdlib_chat_list(list);
        self.spawn(async move {
            let result = functions::load_chats(Some(chat_list), CHAT_PAGE, client_id).await;
            Done::ChatsLoaded(list, result)
        });
    }

    pub(super) fn on_chats_loaded(&mut self, list: ChatListId, result: Result<(), types::Error>) {
        self.loading_lists.remove(&list);
        match result {
            Ok(()) => {}
            Err(err) if err.code == ALL_CHATS_LOADED => {
                self.lists.mark_fully_loaded(list);
            }
            Err(err) => self.emit_error(ErrorContext::ChatList, &err),
        }
        // Always report the list after a load so the UI learns that it finished.
        self.dirty.forced_lists.insert(list);
    }
}
