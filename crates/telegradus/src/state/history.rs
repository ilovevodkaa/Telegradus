//! The message history of the open chat, prepared for rendering.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use telegradus_core::{ChatId, FormattedText, Message, MessageContent, MessageId};

use crate::format;
use crate::rich::{self, Block};

/// A message plus everything derived from it once, when it arrives.
#[derive(Debug, Clone)]
pub struct Item {
    pub message: Message,
    /// Local calendar day, for day separators.
    pub day: NaiveDate,
    /// Local `HH:MM`.
    pub time: String,
    /// Text or caption split into styled blocks.
    pub blocks: Vec<Block>,
    /// The body is one short line and can share a row with the time.
    pub short: bool,
    pub reply: Option<ReplyPreview>,
}

/// Quoted header of a reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyPreview {
    pub sender: String,
    pub text: String,
}

impl Item {
    pub fn new(message: Message) -> Self {
        let time = format::local_datetime(message.date);
        let blocks = body(&message.content).map(rich::blocks).unwrap_or_default();
        let short = matches!(message.content, MessageContent::Text(_))
            && body(&message.content).is_some_and(|t| rich::is_short(&t.text, &blocks));
        Self {
            day: time.date(),
            time: format::time_hm(time),
            blocks,
            short,
            reply: None,
            message,
        }
    }

    pub fn id(&self) -> MessageId {
        self.message.id
    }

    /// The source text the blocks' ranges point into.
    pub fn text(&self) -> &str {
        body(&self.message.content).map_or("", |t| t.text.as_str())
    }

    fn preview(&self) -> ReplyPreview {
        let sender = if self.message.is_outgoing {
            "Вы".to_owned()
        } else {
            self.message.sender_name.clone()
        };
        ReplyPreview {
            sender,
            text: format::truncate_to_width(
                &format::content_preview(&self.message.content),
                320.0,
                13.0,
            )
            .into_owned(),
        }
    }
}

/// The formatted text shown in a message bubble, if the content has one.
pub fn body(content: &MessageContent) -> Option<&FormattedText> {
    match content {
        MessageContent::Text(text) => Some(text),
        MessageContent::Photo { caption, .. }
        | MessageContent::Document { caption, .. }
        | MessageContent::Video { caption, .. }
        | MessageContent::Animation { caption }
        | MessageContent::Voice { caption, .. } => (!caption.text.is_empty()).then_some(caption),
        _ => None,
    }
}

/// State of the chat shown in the chat pane.
#[derive(Debug)]
pub struct OpenChat {
    pub id: ChatId,
    /// Oldest first.
    pub items: Vec<Item>,
    /// The initial page arrived.
    pub loaded: bool,
    pub has_more_older: bool,
    pub loading_older: bool,
    /// The history scrollable reported a viewport (its content overflows).
    pub viewport_seen: bool,
    pub hovered: Option<MessageId>,
    revealed: HashSet<MessageId>,
    viewed: HashSet<MessageId>,
}

impl OpenChat {
    pub fn new(id: ChatId) -> Self {
        Self {
            id,
            items: Vec::new(),
            loaded: false,
            has_more_older: true,
            loading_older: false,
            viewport_seen: false,
            hovered: None,
            revealed: HashSet::new(),
            viewed: HashSet::new(),
        }
    }

    /// Applies a page of history (oldest first).
    pub fn apply_page(&mut self, messages: Vec<Message>, is_initial: bool, has_more_older: bool) {
        if is_initial {
            self.items = messages.into_iter().map(Item::new).collect();
        } else {
            let known: HashSet<MessageId> = self.items.iter().map(Item::id).collect();
            let mut older: Vec<Item> = messages
                .into_iter()
                .filter(|m| !known.contains(&m.id))
                .map(Item::new)
                .collect();
            older.append(&mut self.items);
            self.items = older;
        }
        self.loaded = true;
        self.has_more_older = has_more_older;
        self.loading_older = false;
        self.resolve_replies();
    }

    /// Adds a new message, or replaces it if it is already shown.
    pub fn push(&mut self, message: Message) {
        if let Some(index) = self.position(message.id) {
            self.items[index] = Item::new(message);
        } else {
            let date = message.date;
            let at = self
                .items
                .iter()
                .rposition(|item| item.message.date <= date)
                .map_or(0, |i| i + 1);
            self.items.insert(at, Item::new(message));
        }
        self.resolve_replies();
    }

    /// Replaces an existing message (edits, failed sends).
    pub fn replace(&mut self, message: Message) {
        if let Some(index) = self.position(message.id) {
            let reply = self.items[index].reply.take();
            self.items[index] = Item::new(message);
            self.items[index].reply = reply;
        }
    }

    /// A pending message got its final id.
    pub fn sent(&mut self, old_id: MessageId, message: Message) {
        match self.position(old_id) {
            Some(index) => {
                let reply = self.items[index].reply.take();
                self.items[index] = Item::new(message);
                self.items[index].reply = reply;
            }
            None => self.push(message),
        }
    }

    pub fn delete(&mut self, ids: &[MessageId]) {
        self.items.retain(|item| !ids.contains(&item.id()));
    }

    pub fn oldest_id(&self) -> Option<MessageId> {
        self.items.first().map(Item::id)
    }

    pub fn find(&self, id: MessageId) -> Option<&Item> {
        self.position(id).map(|i| &self.items[i])
    }

    /// Records that a message was shown; returns `true` the first time.
    pub fn mark_viewed(&mut self, id: MessageId) -> bool {
        self.viewed.insert(id)
    }

    pub fn is_viewed(&self, id: MessageId) -> bool {
        self.viewed.contains(&id)
    }

    pub fn toggle_spoiler(&mut self, id: MessageId) {
        if !self.revealed.remove(&id) {
            self.revealed.insert(id);
        }
    }

    pub fn is_revealed(&self, id: MessageId) -> bool {
        self.revealed.contains(&id)
    }

    fn position(&self, id: MessageId) -> Option<usize> {
        // Recent messages change most often: search from the end.
        self.items.iter().rposition(|item| item.id() == id)
    }

    fn resolve_replies(&mut self) {
        let pending = self
            .items
            .iter()
            .any(|item| item.message.reply_to.is_some() && item.reply.is_none());
        if !pending {
            return;
        }
        let index: HashMap<MessageId, usize> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.id(), i))
            .collect();
        let previews: Vec<(usize, ReplyPreview)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.reply.is_none())
            .filter_map(|(i, item)| {
                let target = index.get(&item.message.reply_to?)?;
                Some((i, self.items[*target].preview()))
            })
            .collect();
        for (i, preview) in previews {
            self.items[i].reply = Some(preview);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use telegradus_core::{Sender, SendingState};

    fn message(id: MessageId, date: i64, text: &str, reply_to: Option<MessageId>) -> Message {
        Message {
            id,
            chat_id: 1,
            sender: Sender::User(7),
            sender_name: "Анна".into(),
            date,
            edit_date: 0,
            is_outgoing: false,
            sending_state: SendingState::Sent,
            reply_to,
            content: MessageContent::Text(FormattedText {
                text: text.into(),
                entities: vec![],
            }),
        }
    }

    fn ids(chat: &OpenChat) -> Vec<MessageId> {
        chat.items.iter().map(Item::id).collect()
    }

    #[test]
    fn pages_prepend_and_dedupe() {
        let mut chat = OpenChat::new(1);
        chat.apply_page(
            vec![message(3, 30, "c", None), message(4, 40, "d", None)],
            true,
            true,
        );
        chat.apply_page(
            vec![
                message(1, 10, "a", None),
                message(2, 20, "b", None),
                message(3, 30, "c", None),
            ],
            false,
            false,
        );
        assert_eq!(ids(&chat), vec![1, 2, 3, 4]);
        assert!(!chat.has_more_older);
        assert_eq!(chat.oldest_id(), Some(1));
    }

    #[test]
    fn replies_resolve_when_target_arrives() {
        let mut chat = OpenChat::new(1);
        chat.apply_page(vec![message(5, 50, "ответ", Some(2))], true, true);
        assert!(chat.items[0].reply.is_none());
        chat.apply_page(
            vec![message(2, 20, "вопрос\nвторая строка", None)],
            false,
            true,
        );
        let reply = chat.find(5).and_then(|i| i.reply.clone());
        assert_eq!(
            reply,
            Some(ReplyPreview {
                sender: "Анна".into(),
                text: "вопрос вторая строка".into()
            })
        );
    }

    #[test]
    fn pending_then_sent() {
        let mut chat = OpenChat::new(1);
        chat.apply_page(vec![message(1, 10, "a", None)], true, false);
        let mut pending = message(1_000_001, 20, "b", None);
        pending.sending_state = SendingState::Pending;
        chat.push(pending);
        assert_eq!(ids(&chat), vec![1, 1_000_001]);
        chat.sent(1_000_001, message(2, 20, "b", None));
        assert_eq!(ids(&chat), vec![1, 2]);
        assert_eq!(chat.items[1].message.sending_state, SendingState::Sent);
    }

    #[test]
    fn push_keeps_date_order_and_replaces() {
        let mut chat = OpenChat::new(1);
        chat.apply_page(
            vec![message(1, 10, "a", None), message(3, 30, "c", None)],
            true,
            false,
        );
        chat.push(message(2, 20, "b", None));
        assert_eq!(ids(&chat), vec![1, 2, 3]);
        chat.push(message(2, 20, "b2", None));
        assert_eq!(ids(&chat), vec![1, 2, 3]);
        assert_eq!(chat.items[1].text(), "b2");
    }

    #[test]
    fn delete_and_view_tracking() {
        let mut chat = OpenChat::new(1);
        chat.apply_page(
            vec![message(1, 10, "a", None), message(2, 20, "b", None)],
            true,
            false,
        );
        chat.delete(&[1]);
        assert_eq!(ids(&chat), vec![2]);
        assert!(chat.mark_viewed(2));
        assert!(!chat.mark_viewed(2));
        chat.toggle_spoiler(2);
        assert!(chat.is_revealed(2));
        chat.toggle_spoiler(2);
        assert!(!chat.is_revealed(2));
    }
}
