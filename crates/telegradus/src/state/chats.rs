//! Chat summaries, ordered chat lists and folders as reported by the core.

use std::collections::HashMap;
use std::sync::Arc;

use telegradus_core::{ChatId, ChatListEntry, ChatListId, ChatSummary, Folder};

use crate::format;

/// A chat summary with the derived data the list needs on every frame.
#[derive(Debug, Clone)]
pub struct Chat {
    pub summary: Arc<ChatSummary>,
    pub initials: String,
    pub shade: usize,
}

/// Loading state of one chat list.
#[derive(Debug, Clone)]
pub struct ChatList {
    pub entries: Arc<[ChatListEntry]>,
    pub has_more: bool,
    /// A `LoadChats` is in flight.
    pub loading: bool,
}

impl Default for ChatList {
    fn default() -> Self {
        Self {
            entries: Arc::from([]),
            has_more: true,
            loading: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct Chats {
    chats: HashMap<ChatId, Chat>,
    lists: HashMap<ChatListId, ChatList>,
    folders: Arc<[Folder]>,
    /// Unmuted chats with unread messages per list, for the folder tabs.
    unread: HashMap<ChatListId, i32>,
}

impl Chats {
    pub fn upsert(&mut self, summary: Arc<ChatSummary>) {
        match self.chats.get_mut(&summary.id) {
            Some(chat) => {
                if chat.summary.title != summary.title {
                    chat.initials = format::initials(&summary.title);
                }
                chat.summary = summary;
            }
            None => {
                let chat = Chat {
                    initials: format::initials(&summary.title),
                    shade: format::shade_index(summary.id),
                    summary,
                };
                self.chats.insert(chat.summary.id, chat);
            }
        }
    }

    pub fn get(&self, id: ChatId) -> Option<&Chat> {
        self.chats.get(&id)
    }

    pub fn set_list(&mut self, id: ChatListId, entries: Arc<[ChatListEntry]>, has_more: bool) {
        let list = self.lists.entry(id).or_default();
        list.entries = entries;
        list.has_more = has_more;
        list.loading = false;
    }

    pub fn list(&self, id: ChatListId) -> Option<&ChatList> {
        self.lists.get(&id)
    }

    /// Marks a list as loading; returns `false` if a load is already in flight
    /// or everything is loaded.
    pub fn begin_load(&mut self, id: ChatListId) -> bool {
        let list = self.lists.entry(id).or_default();
        if list.loading || !list.has_more {
            return false;
        }
        list.loading = true;
        true
    }

    /// Whether a list has never been received nor requested.
    pub fn is_untouched(&self, id: ChatListId) -> bool {
        !self.lists.contains_key(&id)
    }

    pub fn loading_failed(&mut self) {
        for list in self.lists.values_mut() {
            list.loading = false;
        }
    }

    pub fn set_folders(&mut self, folders: Arc<[Folder]>) {
        self.folders = folders;
    }

    pub fn folders(&self) -> &[Folder] {
        &self.folders
    }

    /// Unmuted unread chats of a list, as of the last [`Self::refresh_unread`].
    pub fn unread_chats(&self, id: ChatListId) -> i32 {
        self.unread.get(&id).copied().unwrap_or(0)
    }

    /// Recounts unread chats per list; call once after a batch of changes.
    pub fn refresh_unread(&mut self) {
        let chats = &self.chats;
        self.unread = self
            .lists
            .iter()
            .map(|(id, list)| {
                let count = list
                    .entries
                    .iter()
                    .filter_map(|e| chats.get(&e.chat_id))
                    .filter(|c| !c.summary.is_muted && c.summary.unread_count > 0)
                    .count();
                (*id, i32::try_from(count).unwrap_or(i32::MAX))
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use telegradus_core::ChatKind;

    fn summary(id: ChatId, title: &str) -> Arc<ChatSummary> {
        Arc::new(ChatSummary {
            id,
            title: title.into(),
            kind: ChatKind::BasicGroup,
            photo: None,
            last_message: None,
            unread_count: 0,
            unread_mention_count: 0,
            is_muted: false,
            draft: None,
            can_send_messages: true,
            last_read_inbox_message_id: 0,
        })
    }

    #[test]
    fn upsert_refreshes_initials() {
        let mut chats = Chats::default();
        chats.upsert(summary(1, "Анна"));
        assert_eq!(chats.get(1).map(|c| c.initials.as_str()), Some("А"));
        chats.upsert(summary(1, "Борис Петров"));
        assert_eq!(chats.get(1).map(|c| c.initials.as_str()), Some("БП"));
    }

    #[test]
    fn unread_counts_skip_muted_chats() {
        let mut chats = Chats::default();
        for (id, unread, muted) in [(1, 3, false), (2, 5, true), (3, 0, false)] {
            let mut chat = (*summary(id, "x")).clone();
            chat.unread_count = unread;
            chat.is_muted = muted;
            chats.upsert(Arc::new(chat));
        }
        let entries: Arc<[ChatListEntry]> = [1, 2, 3]
            .map(|chat_id| ChatListEntry {
                chat_id,
                is_pinned: false,
            })
            .into();
        chats.set_list(ChatListId::Main, entries, false);
        assert_eq!(chats.unread_chats(ChatListId::Main), 0);
        chats.refresh_unread();
        assert_eq!(chats.unread_chats(ChatListId::Main), 1);
    }

    #[test]
    fn loading_is_single_flight() {
        let mut chats = Chats::default();
        assert!(chats.is_untouched(ChatListId::Main));
        assert!(chats.begin_load(ChatListId::Main));
        assert!(!chats.begin_load(ChatListId::Main));
        chats.set_list(ChatListId::Main, Arc::from([]), true);
        assert!(chats.begin_load(ChatListId::Main));
        chats.set_list(ChatListId::Main, Arc::from([]), false);
        assert!(!chats.begin_load(ChatListId::Main));
    }
}
