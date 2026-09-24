//! Ordered chat lists built from TDLib chat positions.
//!
//! A chat belongs to a list iff its position there has a non-zero order.
//! Lists are sorted by order (descending), then by chat id (descending).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::model::{ChatId, ChatListEntry, ChatListId};

/// A chat's position in one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Position {
    pub list: ChatListId,
    pub order: i64,
    pub is_pinned: bool,
}

#[derive(Debug)]
struct List {
    /// `(order, chat_id)`; iterated in reverse for display order.
    sorted: BTreeSet<(i64, ChatId)>,
    /// Current `(order, is_pinned)` of each chat in the list.
    members: HashMap<ChatId, (i64, bool)>,
    /// `false` once TDLib reported that every chat of the list is loaded.
    has_more: bool,
}

impl Default for List {
    fn default() -> Self {
        Self {
            sorted: BTreeSet::new(),
            members: HashMap::new(),
            has_more: true,
        }
    }
}

/// All chat lists of the current account.
#[derive(Debug, Default)]
pub(crate) struct ChatLists {
    lists: HashMap<ChatListId, List>,
}

impl ChatLists {
    /// Applies one position update. Returns whether the list changed.
    pub fn set_position(&mut self, chat_id: ChatId, position: Position) -> bool {
        let list = self.lists.entry(position.list).or_default();
        let current = list.members.get(&chat_id).copied();
        let wanted = (position.order != 0).then_some((position.order, position.is_pinned));
        if current == wanted {
            return false;
        }
        if let Some((order, _)) = current {
            list.sorted.remove(&(order, chat_id));
            list.members.remove(&chat_id);
        }
        if let Some((order, pinned)) = wanted {
            list.sorted.insert((order, chat_id));
            list.members.insert(chat_id, (order, pinned));
        }
        true
    }

    /// Replaces every position of a chat: lists missing from `positions` lose
    /// the chat. Lists that changed are appended to `changed`.
    pub fn replace_positions(
        &mut self,
        chat_id: ChatId,
        positions: &[Position],
        changed: &mut impl Extend<ChatListId>,
    ) {
        let stale: Vec<ChatListId> = self
            .lists
            .iter()
            .filter(|(id, list)| {
                list.members.contains_key(&chat_id) && !positions.iter().any(|p| p.list == **id)
            })
            .map(|(id, _)| *id)
            .collect();
        for list in stale {
            let removal = Position {
                list,
                order: 0,
                is_pinned: false,
            };
            if self.set_position(chat_id, removal) {
                changed.extend([list]);
            }
        }
        for position in positions {
            if self.set_position(chat_id, *position) {
                changed.extend([position.list]);
            }
        }
    }

    /// The list in display order.
    pub fn entries(&self, list: ChatListId) -> Arc<[ChatListEntry]> {
        match self.lists.get(&list) {
            Some(list) => list
                .sorted
                .iter()
                .rev()
                .map(|(_, chat_id)| ChatListEntry {
                    chat_id: *chat_id,
                    is_pinned: list.members.get(chat_id).is_some_and(|(_, pinned)| *pinned),
                })
                .collect(),
            None => Arc::from([]),
        }
    }

    pub fn has_more(&self, list: ChatListId) -> bool {
        self.lists.get(&list).is_none_or(|l| l.has_more)
    }

    /// Records that TDLib has no more chats for `list`. Returns whether this changed anything.
    pub fn mark_fully_loaded(&mut self, list: ChatListId) -> bool {
        let list = self.lists.entry(list).or_default();
        std::mem::replace(&mut list.has_more, false)
    }

    pub fn clear(&mut self) {
        self.lists.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(list: ChatListId, order: i64, is_pinned: bool) -> Position {
        Position {
            list,
            order,
            is_pinned,
        }
    }

    fn ids(lists: &ChatLists, list: ChatListId) -> Vec<ChatId> {
        lists.entries(list).iter().map(|e| e.chat_id).collect()
    }

    #[test]
    fn sorted_by_order_then_chat_id_descending() {
        let mut lists = ChatLists::default();
        assert!(lists.set_position(1, pos(ChatListId::Main, 10, false)));
        assert!(lists.set_position(2, pos(ChatListId::Main, 30, false)));
        assert!(lists.set_position(3, pos(ChatListId::Main, 10, false)));
        assert!(lists.set_position(-4, pos(ChatListId::Main, 20, false)));
        assert_eq!(ids(&lists, ChatListId::Main), [2, -4, 3, 1]);
    }

    #[test]
    fn reordering_pinning_and_removal() {
        let mut lists = ChatLists::default();
        lists.set_position(1, pos(ChatListId::Main, 10, false));
        lists.set_position(2, pos(ChatListId::Main, 20, false));
        assert!(!lists.set_position(2, pos(ChatListId::Main, 20, false)));

        assert!(lists.set_position(1, pos(ChatListId::Main, 99, true)));
        let entries = lists.entries(ChatListId::Main);
        assert_eq!(
            entries[..],
            [
                ChatListEntry {
                    chat_id: 1,
                    is_pinned: true
                },
                ChatListEntry {
                    chat_id: 2,
                    is_pinned: false
                },
            ]
        );

        assert!(lists.set_position(1, pos(ChatListId::Main, 0, false)));
        assert_eq!(ids(&lists, ChatListId::Main), [2]);
        assert!(!lists.set_position(1, pos(ChatListId::Main, 0, false)));
    }

    #[test]
    fn lists_are_independent() {
        let mut lists = ChatLists::default();
        lists.set_position(1, pos(ChatListId::Main, 10, false));
        lists.set_position(1, pos(ChatListId::Folder(3), 5, true));
        lists.set_position(2, pos(ChatListId::Archive, 7, false));
        assert_eq!(ids(&lists, ChatListId::Main), [1]);
        assert_eq!(ids(&lists, ChatListId::Folder(3)), [1]);
        assert_eq!(ids(&lists, ChatListId::Archive), [2]);
        assert!(lists.entries(ChatListId::Folder(9)).is_empty());
    }

    #[test]
    fn replace_positions_removes_missing_lists() {
        let mut lists = ChatLists::default();
        lists.set_position(1, pos(ChatListId::Main, 10, false));
        lists.set_position(1, pos(ChatListId::Folder(3), 5, false));
        lists.set_position(2, pos(ChatListId::Folder(3), 6, false));

        let mut changed = Vec::new();
        lists.replace_positions(1, &[pos(ChatListId::Archive, 4, false)], &mut changed);
        changed.sort();
        assert_eq!(
            changed,
            [ChatListId::Main, ChatListId::Archive, ChatListId::Folder(3)]
        );
        assert!(ids(&lists, ChatListId::Main).is_empty());
        assert_eq!(ids(&lists, ChatListId::Folder(3)), [2]);
        assert_eq!(ids(&lists, ChatListId::Archive), [1]);

        let mut changed = Vec::new();
        lists.replace_positions(1, &[pos(ChatListId::Archive, 4, false)], &mut changed);
        assert!(changed.is_empty());
    }

    #[test]
    fn has_more_until_fully_loaded() {
        let mut lists = ChatLists::default();
        assert!(lists.has_more(ChatListId::Main));
        assert!(lists.mark_fully_loaded(ChatListId::Main));
        assert!(!lists.mark_fully_loaded(ChatListId::Main));
        assert!(!lists.has_more(ChatListId::Main));
        assert!(lists.has_more(ChatListId::Archive));
        lists.clear();
        assert!(lists.has_more(ChatListId::Main));
    }
}
