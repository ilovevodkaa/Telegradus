//! Message history of open chats: page loading and a bounded message cache.

use std::collections::BTreeMap;

use tdlib_rs::{enums, functions, types};

use crate::model::{ChatId, Message, MessageId};

/// Messages requested per history page.
pub(crate) const PAGE_SIZE: usize = 50;
/// Upper bound on `getChatHistory` calls for one page (TDLib may return tiny pages).
const MAX_REQUESTS_PER_PAGE: usize = 10;
/// Messages kept per open chat to turn edit/content updates into full messages.
const MAX_CACHED_MESSAGES: usize = 1500;

/// A page of history, newest first as returned by TDLib.
#[derive(Debug)]
pub(crate) struct Page {
    pub messages: Vec<types::Message>,
    /// `false` when the beginning of the chat was reached.
    pub has_more: bool,
}

/// Loads up to `wanted` messages older than `from_message_id` (0 = newest).
///
/// TDLib often answers with only the locally cached messages first, so the
/// request is repeated from the oldest received message until enough messages
/// arrived or a page comes back empty.
pub(crate) async fn fetch_page(
    client_id: i32,
    chat_id: ChatId,
    from_message_id: MessageId,
    wanted: usize,
) -> Result<Page, types::Error> {
    let mut messages: Vec<types::Message> = Vec::with_capacity(wanted);
    let mut from = from_message_id;
    for _ in 0..MAX_REQUESTS_PER_PAGE {
        let limit =
            i32::try_from(wanted.saturating_sub(messages.len()).clamp(1, 100)).unwrap_or(100);
        let enums::Messages::Messages(page) =
            functions::get_chat_history(chat_id, from, 0, limit, false, client_id).await?;
        let page: Vec<types::Message> = page
            .messages
            .into_iter()
            .flatten()
            .filter(|m| from == 0 || m.id < from)
            .collect();
        let Some(oldest) = page.iter().map(|m| m.id).min() else {
            return Ok(Page {
                messages,
                has_more: false,
            });
        };
        from = oldest;
        messages.extend(page);
        if messages.len() >= wanted {
            break;
        }
    }
    Ok(Page {
        messages,
        has_more: true,
    })
}

/// State of a chat the UI has opened.
#[derive(Debug, Default)]
pub(crate) struct OpenChat {
    messages: BTreeMap<MessageId, Message>,
    /// A `LoadOlder` request is in flight.
    pub loading_older: bool,
}

impl OpenChat {
    /// Caches a message, evicting the oldest ones beyond the bound.
    pub fn insert(&mut self, message: Message) {
        self.messages.insert(message.id, message);
        while self.messages.len() > MAX_CACHED_MESSAGES {
            self.messages.pop_first();
        }
    }

    pub fn get(&self, id: MessageId) -> Option<&Message> {
        self.messages.get(&id)
    }

    pub fn get_mut(&mut self, id: MessageId) -> Option<&mut Message> {
        self.messages.get_mut(&id)
    }

    pub fn remove(&mut self, id: MessageId) -> Option<Message> {
        self.messages.remove(&id)
    }

    /// Every cached message, oldest first.
    pub fn messages(&self) -> Vec<Message> {
        self.messages.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MessageContent, Sender, SendingState};
    use crate::text::plain;

    fn message(id: MessageId) -> Message {
        Message {
            id,
            chat_id: 1,
            sender: Sender::User(1),
            sender_name: "A".into(),
            date: 0,
            edit_date: 0,
            is_outgoing: false,
            sending_state: SendingState::Sent,
            reply_to: None,
            content: MessageContent::Text(plain(id.to_string())),
        }
    }

    #[test]
    fn cache_is_bounded_and_ordered() {
        let mut chat = OpenChat::default();
        for id in (1..=MAX_CACHED_MESSAGES as i64 + 10).rev() {
            chat.insert(message(id));
        }
        let messages = chat.messages();
        assert_eq!(messages.len(), MAX_CACHED_MESSAGES);
        assert_eq!(messages[0].id, 11);
        assert!(messages.windows(2).all(|w| w[0].id < w[1].id));
        assert!(chat.get(5).is_none());
        assert!(chat.remove(11).is_some());
        assert!(chat.get_mut(11).is_none());
    }
}
