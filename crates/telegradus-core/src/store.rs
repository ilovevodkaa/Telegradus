//! Chats, users and groups known to the core, and [`ChatSummary`] building.
//!
//! Only the fields the UI needs are kept, so memory stays proportional to the
//! number of chats and users rather than to the size of TDLib objects.

use std::collections::HashMap;
use std::sync::Arc;

use tdlib_rs::enums::{self, ChatMemberStatus, InputMessageContent, NotificationSettingsScope};
use tdlib_rs::types;

use crate::convert::{chat_kind, file_ref};
use crate::message::{self, Author, Names, join_name, preview_text};
use crate::model::{
    ChatId, ChatKind, ChatSummary, FileId, FileRef, MessageContent, MessageId, MessagePreview,
    Sender, UserId,
};

/// The last message of a chat, reduced to what the chat list shows.
#[derive(Debug, Clone)]
struct LastMessage {
    id: MessageId,
    date: i64,
    is_outgoing: bool,
    sender: Sender,
    is_service: bool,
    text: String,
}

#[derive(Debug)]
struct ChatState {
    id: ChatId,
    title: String,
    kind: ChatKind,
    group: Option<GroupRef>,
    photo: Option<FileRef>,
    last_message: Option<LastMessage>,
    unread_count: i32,
    unread_mention_count: i32,
    last_read_inbox_message_id: MessageId,
    use_default_mute_for: bool,
    mute_for: i32,
    draft: Option<String>,
    /// `can_send_basic_messages` of the chat's default permissions.
    can_send_basic: bool,
    /// The summary most recently sent to the UI.
    sent: Option<Arc<ChatSummary>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GroupRef {
    Basic(i64),
    Super(i64),
}

#[derive(Debug, PartialEq, Eq)]
struct UserInfo {
    first_name: String,
    last_name: String,
    is_deleted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Membership {
    Owner,
    Admin { can_post: bool },
    Member,
    Restricted { can_send: bool },
    Left,
}

#[derive(Debug, Clone, Copy)]
struct GroupInfo {
    membership: Membership,
    /// `false` for basic groups upgraded to supergroups.
    is_active: bool,
}

/// Everything the core knows about chats of the current account.
#[derive(Debug, Default)]
pub(crate) struct Store {
    chats: HashMap<ChatId, ChatState>,
    users: HashMap<UserId, UserInfo>,
    groups: HashMap<GroupRef, GroupInfo>,
    /// Reverse indexes used to refresh summaries when related objects change.
    group_chats: HashMap<GroupRef, ChatId>,
    user_chats: HashMap<UserId, ChatId>,
    photo_files: HashMap<FileId, ChatId>,
    /// Default `mute_for` of private chats, groups and channels.
    scope_mute_for: [i32; 3],
    my_id: Option<UserId>,
}

impl Store {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn contains_chat(&self, chat_id: ChatId) -> bool {
        self.chats.contains_key(&chat_id)
    }

    pub fn my_id(&self) -> Option<UserId> {
        self.my_id
    }

    pub fn set_my_id(&mut self, id: UserId) {
        self.my_id = Some(id);
    }

    /// Adds (or replaces) a chat from `updateNewChat`.
    pub fn add_chat(&mut self, chat: types::Chat) {
        let kind = chat_kind(&chat.r#type);
        let group = match &chat.r#type {
            enums::ChatType::BasicGroup(g) => Some(GroupRef::Basic(g.basic_group_id)),
            enums::ChatType::Supergroup(g) => Some(GroupRef::Super(g.supergroup_id)),
            enums::ChatType::Private(_) | enums::ChatType::Secret(_) => None,
        };
        match (group, kind) {
            (Some(group), _) => {
                self.group_chats.insert(group, chat.id);
            }
            (None, ChatKind::Private { user_id }) => {
                self.user_chats.insert(user_id, chat.id);
            }
            _ => {}
        }
        let settings = &chat.notification_settings;
        let state = ChatState {
            id: chat.id,
            title: chat.title,
            kind,
            group,
            photo: None,
            last_message: None,
            unread_count: chat.unread_count,
            unread_mention_count: chat.unread_mention_count,
            last_read_inbox_message_id: chat.last_read_inbox_message_id,
            use_default_mute_for: settings.use_default_mute_for,
            mute_for: settings.mute_for,
            draft: draft_text(chat.draft_message.as_ref()),
            can_send_basic: chat.permissions.can_send_basic_messages,
            sent: None,
        };
        let id = chat.id;
        self.chats.insert(id, state);
        self.set_photo(id, chat.photo.as_ref());
        // Converted after insertion so service texts know whether this is a channel.
        self.set_last_message(id, chat.last_message);
    }

    pub fn set_title(&mut self, chat_id: ChatId, title: String) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.title = title;
        }
    }

    pub fn set_photo(&mut self, chat_id: ChatId, photo: Option<&types::ChatPhotoInfo>) {
        let Some(chat) = self.chats.get_mut(&chat_id) else {
            return;
        };
        if let Some(old) = chat.photo.take() {
            self.photo_files.remove(&old.id);
        }
        if let Some(photo) = photo {
            let file = file_ref(&photo.small);
            self.photo_files.insert(file.id, chat_id);
            chat.photo = Some(file);
        }
    }

    /// Records new state of a file. Returns the chat whose avatar finished
    /// (or lost) its download, if any.
    pub fn update_photo_file(&mut self, file: &FileRef) -> Option<ChatId> {
        let chat_id = *self.photo_files.get(&file.id)?;
        let photo = self
            .chats
            .get_mut(&chat_id)?
            .photo
            .as_mut()
            .filter(|photo| photo.id == file.id)?;
        let completion_changed = photo.is_downloaded() != file.is_downloaded();
        *photo = file.clone();
        completion_changed.then_some(chat_id)
    }

    pub fn set_last_message(&mut self, chat_id: ChatId, message: Option<types::Message>) {
        let last = message.map(|m| self.last_message(m));
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.last_message = last;
        }
    }

    /// Refreshes the preview after `updateMessageContent`. Returns whether it
    /// concerned the chat's last message.
    pub fn update_last_message_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        content: enums::MessageContent,
    ) -> bool {
        let Some(last) = self
            .chats
            .get(&chat_id)
            .and_then(|c| c.last_message.as_ref())
            .filter(|m| m.id == message_id)
        else {
            return false;
        };
        let (sender, is_outgoing) = (last.sender.clone(), last.is_outgoing);
        let content = self.content(content, &sender, chat_id, is_outgoing);
        if let Some(last) = self
            .chats
            .get_mut(&chat_id)
            .and_then(|c| c.last_message.as_mut())
        {
            last.is_service = matches!(content, MessageContent::Service(_));
            last.text = preview_text(&content);
        }
        true
    }

    /// Follows the id change of a sent message that is the chat's last message.
    pub fn rename_last_message(
        &mut self,
        chat_id: ChatId,
        old_id: MessageId,
        new_id: MessageId,
    ) -> bool {
        match self
            .chats
            .get_mut(&chat_id)
            .and_then(|c| c.last_message.as_mut())
        {
            Some(last) if last.id == old_id => {
                last.id = new_id;
                true
            }
            _ => false,
        }
    }

    fn last_message(&self, message: types::Message) -> LastMessage {
        let sender = message::sender(&message.sender_id);
        let content = self.content(
            message.content,
            &sender,
            message.chat_id,
            message.is_outgoing,
        );
        LastMessage {
            id: message.id,
            date: i64::from(message.date),
            is_outgoing: message.is_outgoing,
            is_service: matches!(content, MessageContent::Service(_)),
            text: preview_text(&content),
            sender,
        }
    }

    fn content(
        &self,
        content: enums::MessageContent,
        sender: &Sender,
        chat_id: ChatId,
        is_outgoing: bool,
    ) -> MessageContent {
        let name = message::sender_name(sender, self);
        let author = Author {
            sender,
            name: &name,
            chat_id,
            is_outgoing,
        };
        message::content(content, &author, self)
    }

    pub fn set_permissions(&mut self, chat_id: ChatId, permissions: &types::ChatPermissions) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.can_send_basic = permissions.can_send_basic_messages;
        }
    }

    pub fn set_read_inbox(&mut self, chat_id: ChatId, last_read: MessageId, unread_count: i32) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.last_read_inbox_message_id = last_read;
            chat.unread_count = unread_count;
        }
    }

    pub fn set_unread_mention_count(&mut self, chat_id: ChatId, count: i32) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.unread_mention_count = count;
        }
    }

    pub fn set_notification_settings(
        &mut self,
        chat_id: ChatId,
        settings: &types::ChatNotificationSettings,
    ) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.use_default_mute_for = settings.use_default_mute_for;
            chat.mute_for = settings.mute_for;
        }
    }

    /// Updates a scope default. Returns the chats that follow that default.
    pub fn set_scope_mute_for(
        &mut self,
        scope: &NotificationSettingsScope,
        mute_for: i32,
    ) -> Vec<ChatId> {
        let index = match scope {
            NotificationSettingsScope::PrivateChats => 0,
            NotificationSettingsScope::GroupChats => 1,
            NotificationSettingsScope::ChannelChats => 2,
        };
        self.scope_mute_for[index] = mute_for;
        self.chats
            .values()
            .filter(|c| c.use_default_mute_for && scope_index(c.kind) == index)
            .map(|c| c.id)
            .collect()
    }

    pub fn set_draft(&mut self, chat_id: ChatId, draft: Option<&types::DraftMessage>) {
        if let Some(chat) = self.chats.get_mut(&chat_id) {
            chat.draft = draft_text(draft);
        }
    }

    /// Stores a user. Returns the chats whose summary may depend on it.
    pub fn upsert_user(&mut self, user: &types::User) -> Vec<ChatId> {
        let info = UserInfo {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            is_deleted: matches!(user.r#type, enums::UserType::Deleted),
        };
        let previous = self.users.insert(user.id, info);
        let mut affected: Vec<ChatId> =
            self.user_chats.get(&user.id).copied().into_iter().collect();
        let renamed = previous.is_some_and(|p| Some(&p) != self.users.get(&user.id));
        if renamed {
            // Group previews show the sender's first name. Renames are rare, so a scan is fine.
            let sender = Sender::User(user.id);
            affected.extend(
                self.chats
                    .values()
                    .filter(|c| c.last_message.as_ref().is_some_and(|m| m.sender == sender))
                    .map(|c| c.id),
            );
        }
        affected
    }

    /// Stores a basic group. Returns its chat, if known.
    pub fn upsert_basic_group(&mut self, group: &types::BasicGroup) -> Option<ChatId> {
        let key = GroupRef::Basic(group.id);
        let info = GroupInfo {
            membership: membership(&group.status),
            is_active: group.is_active,
        };
        self.groups.insert(key, info);
        self.group_chats.get(&key).copied()
    }

    /// Stores a supergroup or channel. Returns its chat, if known.
    pub fn upsert_supergroup(&mut self, group: &types::Supergroup) -> Option<ChatId> {
        let key = GroupRef::Super(group.id);
        let info = GroupInfo {
            membership: membership(&group.status),
            is_active: true,
        };
        self.groups.insert(key, info);
        self.group_chats.get(&key).copied()
    }

    /// Builds the chat's summary and returns it if it differs from the last one sent.
    pub fn take_changed_summary(&mut self, chat_id: ChatId) -> Option<Arc<ChatSummary>> {
        let summary = self.summary(self.chats.get(&chat_id)?);
        let chat = self.chats.get_mut(&chat_id)?;
        if chat.sent.as_deref() == Some(&summary) {
            return None;
        }
        let summary = Arc::new(summary);
        chat.sent = Some(Arc::clone(&summary));
        Some(summary)
    }

    fn summary(&self, chat: &ChatState) -> ChatSummary {
        ChatSummary {
            id: chat.id,
            title: chat.title.clone(),
            kind: chat.kind,
            photo: chat.photo.clone(),
            last_message: chat.last_message.as_ref().map(|m| self.preview(chat, m)),
            unread_count: chat.unread_count,
            unread_mention_count: chat.unread_mention_count,
            is_muted: self.is_muted(chat),
            draft: chat.draft.clone(),
            can_send_messages: self.can_send(chat),
            last_read_inbox_message_id: chat.last_read_inbox_message_id,
        }
    }

    fn preview(&self, chat: &ChatState, message: &LastMessage) -> MessagePreview {
        let is_group = matches!(chat.kind, ChatKind::BasicGroup | ChatKind::Supergroup);
        let sender_name = if !is_group || message.is_service {
            None
        } else if message.is_outgoing {
            Some("Вы".to_owned())
        } else {
            match message.sender {
                Sender::User(id) => self.user_first_name(id),
                Sender::Chat(id) if id != chat.id => self.chat_title(id),
                Sender::Chat(_) => None,
            }
        };
        MessagePreview {
            id: message.id,
            date: message.date,
            is_outgoing: message.is_outgoing,
            sender_name,
            text: message.text.clone(),
        }
    }

    fn is_muted(&self, chat: &ChatState) -> bool {
        let mute_for = if chat.use_default_mute_for {
            self.scope_mute_for[scope_index(chat.kind)]
        } else {
            chat.mute_for
        };
        mute_for > 0
    }

    fn can_send(&self, chat: &ChatState) -> bool {
        let group = chat.group.and_then(|g| self.groups.get(&g));
        match chat.kind {
            ChatKind::Private { user_id } => self.users.get(&user_id).is_none_or(|u| !u.is_deleted),
            // Secret chats are disabled in this client.
            ChatKind::Secret { .. } => false,
            ChatKind::BasicGroup | ChatKind::Supergroup => match group {
                None => chat.can_send_basic,
                Some(g) if !g.is_active => false,
                Some(g) => match g.membership {
                    Membership::Owner | Membership::Admin { .. } => true,
                    Membership::Member => chat.can_send_basic,
                    Membership::Restricted { can_send } => can_send && chat.can_send_basic,
                    Membership::Left => false,
                },
            },
            ChatKind::Channel => group.is_some_and(|g| {
                matches!(
                    g.membership,
                    Membership::Owner | Membership::Admin { can_post: true }
                )
            }),
        }
    }
}

impl Names for Store {
    fn user_name(&self, user_id: UserId) -> Option<String> {
        let user = self.users.get(&user_id)?;
        if user.is_deleted {
            return Some("Удалённый аккаунт".to_owned());
        }
        Some(join_name(&user.first_name, &user.last_name)).filter(|n| !n.is_empty())
    }

    fn user_first_name(&self, user_id: UserId) -> Option<String> {
        match self.users.get(&user_id) {
            Some(user) if !user.is_deleted && !user.first_name.trim().is_empty() => {
                Some(user.first_name.trim().to_owned())
            }
            _ => self.user_name(user_id),
        }
    }

    fn chat_title(&self, chat_id: ChatId) -> Option<String> {
        self.chats.get(&chat_id).map(|c| c.title.clone())
    }

    fn is_channel(&self, chat_id: ChatId) -> bool {
        self.chats
            .get(&chat_id)
            .is_some_and(|c| c.kind == ChatKind::Channel)
    }
}

fn scope_index(kind: ChatKind) -> usize {
    match kind {
        ChatKind::Private { .. } | ChatKind::Secret { .. } => 0,
        ChatKind::BasicGroup | ChatKind::Supergroup => 1,
        ChatKind::Channel => 2,
    }
}

fn membership(status: &ChatMemberStatus) -> Membership {
    match status {
        ChatMemberStatus::Creator(c) if c.is_member => Membership::Owner,
        ChatMemberStatus::Administrator(a) => Membership::Admin {
            can_post: a.rights.can_post_messages,
        },
        ChatMemberStatus::Member(_) => Membership::Member,
        ChatMemberStatus::Restricted(r) if r.is_member => Membership::Restricted {
            can_send: r.permissions.can_send_basic_messages,
        },
        ChatMemberStatus::Creator(_)
        | ChatMemberStatus::Restricted(_)
        | ChatMemberStatus::Left
        | ChatMemberStatus::Banned(_) => Membership::Left,
    }
}

fn draft_text(draft: Option<&types::DraftMessage>) -> Option<String> {
    match &draft?.input_message_text {
        InputMessageContent::InputMessageText(input) if !input.text.text.trim().is_empty() => {
            Some(input.text.text.clone())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: ChatId, kind: ChatKind, group: Option<GroupRef>) -> ChatState {
        ChatState {
            id,
            title: format!("chat {id}"),
            kind,
            group,
            photo: None,
            last_message: None,
            unread_count: 0,
            unread_mention_count: 0,
            last_read_inbox_message_id: 0,
            use_default_mute_for: true,
            mute_for: 0,
            draft: None,
            can_send_basic: true,
            sent: None,
        }
    }

    fn last(sender: Sender, is_outgoing: bool, is_service: bool) -> LastMessage {
        LastMessage {
            id: 1,
            date: 100,
            is_outgoing,
            sender,
            is_service,
            text: "текст".into(),
        }
    }

    fn store_with(chats: Vec<ChatState>) -> Store {
        let mut store = Store::default();
        for chat in chats {
            store.chats.insert(chat.id, chat);
        }
        let anna = UserInfo {
            first_name: "Анна".into(),
            last_name: "Иванова".into(),
            is_deleted: false,
        };
        store.users.insert(7, anna);
        store
    }

    fn summary(store: &Store, id: ChatId) -> ChatSummary {
        store.summary(&store.chats[&id])
    }

    fn set_membership(store: &mut Store, key: GroupRef, membership: Membership) {
        let info = GroupInfo {
            membership,
            is_active: true,
        };
        store.groups.insert(key, info);
    }

    #[test]
    fn group_previews_name_the_sender() {
        let mut group = chat(-1, ChatKind::Supergroup, None);
        group.last_message = Some(last(Sender::User(7), false, false));
        let mut private = chat(7, ChatKind::Private { user_id: 7 }, None);
        private.last_message = Some(last(Sender::User(7), false, false));
        let mut store = store_with(vec![group, private]);

        let preview = summary(&store, -1).last_message.unwrap();
        assert_eq!(preview.sender_name.as_deref(), Some("Анна"));
        assert_eq!(preview.text, "текст");
        assert_eq!(summary(&store, 7).last_message.unwrap().sender_name, None);

        let group = store.chats.get_mut(&-1).unwrap();
        group.last_message = Some(last(Sender::User(7), true, false));
        let preview = summary(&store, -1).last_message.unwrap();
        assert_eq!(preview.sender_name.as_deref(), Some("Вы"));

        let group = store.chats.get_mut(&-1).unwrap();
        group.last_message = Some(last(Sender::User(7), false, true));
        assert_eq!(summary(&store, -1).last_message.unwrap().sender_name, None);
    }

    #[test]
    fn mute_follows_scope_defaults_unless_overridden() {
        let mut own = chat(2, ChatKind::Private { user_id: 2 }, None);
        own.use_default_mute_for = false;
        own.mute_for = 3600;
        let mut store = store_with(vec![chat(1, ChatKind::Private { user_id: 1 }, None), own]);
        assert!(!summary(&store, 1).is_muted);
        assert!(summary(&store, 2).is_muted);

        let affected = store.set_scope_mute_for(&NotificationSettingsScope::PrivateChats, 100);
        assert_eq!(affected, vec![1]);
        assert!(summary(&store, 1).is_muted);
        let channels = store.set_scope_mute_for(&NotificationSettingsScope::ChannelChats, 10);
        assert!(channels.is_empty());
    }

    #[test]
    fn posting_rights() {
        let channel = GroupRef::Super(10);
        let group = GroupRef::Super(20);
        let basic = GroupRef::Basic(30);
        let mut store = store_with(vec![
            chat(-10, ChatKind::Channel, Some(channel)),
            chat(-20, ChatKind::Supergroup, Some(group)),
            chat(-30, ChatKind::BasicGroup, Some(basic)),
            chat(7, ChatKind::Private { user_id: 7 }, None),
        ]);

        // Unknown channel membership: reading only.
        assert!(!summary(&store, -10).can_send_messages);
        set_membership(&mut store, channel, Membership::Admin { can_post: false });
        assert!(!summary(&store, -10).can_send_messages);
        set_membership(&mut store, channel, Membership::Admin { can_post: true });
        assert!(summary(&store, -10).can_send_messages);

        set_membership(&mut store, group, Membership::Member);
        assert!(summary(&store, -20).can_send_messages);
        store.chats.get_mut(&-20).unwrap().can_send_basic = false;
        assert!(!summary(&store, -20).can_send_messages);
        set_membership(&mut store, group, Membership::Owner);
        assert!(summary(&store, -20).can_send_messages);
        set_membership(&mut store, group, Membership::Left);
        assert!(!summary(&store, -20).can_send_messages);

        set_membership(
            &mut store,
            basic,
            Membership::Restricted { can_send: false },
        );
        assert!(!summary(&store, -30).can_send_messages);

        assert!(summary(&store, 7).can_send_messages);
        store.users.get_mut(&7).unwrap().is_deleted = true;
        assert!(!summary(&store, 7).can_send_messages);
    }

    #[test]
    fn unchanged_summaries_are_not_resent() {
        let mut store = store_with(vec![chat(1, ChatKind::BasicGroup, None)]);
        assert!(store.take_changed_summary(1).is_some());
        assert!(store.take_changed_summary(1).is_none());
        store.set_unread_mention_count(1, 3);
        let summary = store.take_changed_summary(1).unwrap();
        assert_eq!(summary.unread_mention_count, 3);
        assert!(store.take_changed_summary(404).is_none());
    }

    #[test]
    fn names_for_messages() {
        let mut store = store_with(vec![chat(-5, ChatKind::Channel, None)]);
        assert_eq!(store.user_name(7).as_deref(), Some("Анна Иванова"));
        assert_eq!(store.user_first_name(7).as_deref(), Some("Анна"));
        assert_eq!(store.user_name(8), None);
        assert_eq!(store.chat_title(-5).as_deref(), Some("chat -5"));
        assert!(store.is_channel(-5));
        store.users.get_mut(&7).unwrap().is_deleted = true;
        assert_eq!(store.user_name(7).as_deref(), Some("Удалённый аккаунт"));
    }
}
