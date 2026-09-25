//! TDLib message to [`Message`] conversion and chat-list previews.

use tdlib_rs::{enums, types};

use crate::convert::file_ref;
use crate::errors;
use crate::model::{ChatId, FormattedText, Message, MessageContent, Sender, SendingState, UserId};
use crate::text::{formatted_text, plain};
use crate::{ru, service};

/// Name resolution for senders and service messages.
pub(crate) trait Names {
    /// Full display name of a user, if the user is known.
    fn user_name(&self, user_id: UserId) -> Option<String>;
    /// First name (or the full name when it is empty), for compact previews.
    fn user_first_name(&self, user_id: UserId) -> Option<String>;
    fn chat_title(&self, chat_id: ChatId) -> Option<String>;
    fn is_channel(&self, chat_id: ChatId) -> bool;
}

/// Maximum length (in chars) of a chat-list preview.
const PREVIEW_CHARS: usize = 160;

pub(crate) fn sender(sender: &enums::MessageSender) -> Sender {
    match sender {
        enums::MessageSender::User(user) => Sender::User(user.user_id),
        enums::MessageSender::Chat(chat) => Sender::Chat(chat.chat_id),
    }
}

/// Display name of a message sender.
pub(crate) fn sender_name(sender: &Sender, names: &impl Names) -> String {
    match sender {
        Sender::User(id) => names
            .user_name(*id)
            .unwrap_or_else(|| "Пользователь".to_owned()),
        Sender::Chat(id) => names.chat_title(*id).unwrap_or_else(|| "Чат".to_owned()),
    }
}

pub(crate) fn sending_state(state: Option<&enums::MessageSendingState>) -> SendingState {
    match state {
        None => SendingState::Sent,
        Some(enums::MessageSendingState::Pending(_)) => SendingState::Pending,
        Some(enums::MessageSendingState::Failed(failed)) => SendingState::Failed {
            error: errors::describe(failed.error.code, &failed.error.message),
        },
    }
}

/// Converts a full TDLib message, resolving the sender's name.
pub(crate) fn message(message: types::Message, names: &impl Names) -> Message {
    let sender = sender(&message.sender_id);
    let sender_name = sender_name(&sender, names);
    let reply_to = match &message.reply_to {
        Some(enums::MessageReplyTo::Message(reply))
            if reply.chat_id == message.chat_id && reply.message_id != 0 =>
        {
            Some(reply.message_id)
        }
        _ => None,
    };
    let content = content(
        message.content,
        &Author {
            sender: &sender,
            name: &sender_name,
            chat_id: message.chat_id,
            is_outgoing: message.is_outgoing,
        },
        names,
    );
    Message {
        id: message.id,
        chat_id: message.chat_id,
        sending_state: sending_state(message.sending_state.as_ref()),
        sender,
        sender_name,
        date: i64::from(message.date),
        edit_date: i64::from(message.edit_date),
        is_outgoing: message.is_outgoing,
        reply_to,
        content,
    }
}

/// Who sent a message, for service texts.
pub(crate) struct Author<'a> {
    pub sender: &'a Sender,
    pub name: &'a str,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
}

impl Author<'_> {
    pub fn user_id(&self) -> Option<UserId> {
        match self.sender {
            Sender::User(id) => Some(*id),
            Sender::Chat(_) => None,
        }
    }
}

/// Converts message content. Media is kept; service messages are rendered as
/// Russian text; everything else becomes a labelled [`MessageContent::Unsupported`].
pub(crate) fn content(
    content: enums::MessageContent,
    author: &Author<'_>,
    names: &impl Names,
) -> MessageContent {
    use enums::MessageContent as C;
    let unsupported = |label: &str| MessageContent::Unsupported(label.to_owned());
    match content {
        C::MessageText(m) => MessageContent::Text(formatted_text(m.text)),
        C::MessagePhoto(m) => match best_photo_size(&m.photo.sizes) {
            Some(size) => MessageContent::Photo {
                file: file_ref(&size.photo),
                width: size.width,
                height: size.height,
                caption: formatted_text(m.caption),
            },
            None => unsupported("Фото"),
        },
        C::MessageDocument(m) => MessageContent::Document {
            file: file_ref(&m.document.document),
            file_name: m.document.file_name,
            caption: formatted_text(m.caption),
        },
        C::MessageVideo(m) => MessageContent::Video {
            duration: m.video.duration,
            caption: formatted_text(m.caption),
        },
        C::MessageAnimation(m) => MessageContent::Animation {
            caption: formatted_text(m.caption),
        },
        C::MessageAudio(m) => MessageContent::Audio {
            title: m.audio.title,
            performer: m.audio.performer,
            duration: m.audio.duration,
        },
        C::MessageVoiceNote(m) => MessageContent::Voice {
            duration: m.voice_note.duration,
            caption: formatted_text(m.caption),
        },
        C::MessageVideoNote(m) => MessageContent::VideoNote {
            duration: m.video_note.duration,
        },
        C::MessageSticker(m) => MessageContent::Sticker {
            emoji: m.sticker.emoji,
        },
        C::MessageAnimatedEmoji(m) => MessageContent::Text(plain(m.emoji)),
        C::MessageDice(m) if m.value > 0 => {
            MessageContent::Text(plain(format!("{} {}", m.emoji, m.value)))
        }
        C::MessageDice(m) => MessageContent::Text(plain(m.emoji)),
        C::MessagePaidMedia(_) => unsupported("Платный контент"),
        C::MessageExpiredPhoto => unsupported("Фото с таймером"),
        C::MessageExpiredVideo => unsupported("Видео с таймером"),
        C::MessageExpiredVideoNote => unsupported("Видеосообщение с таймером"),
        C::MessageExpiredVoiceNote => unsupported("Голосовое сообщение с таймером"),
        C::MessageLocation(_) => unsupported("Геопозиция"),
        C::MessageVenue(m) => labelled("Место", &m.venue.title),
        C::MessageContact(m) => {
            let name = join_name(&m.contact.first_name, &m.contact.last_name);
            labelled("Контакт", &name)
        }
        C::MessageGame(m) => labelled("Игра", &m.game.title),
        C::MessagePoll(m) => labelled("Опрос", &m.poll.question.text),
        C::MessageStakeDice(_) => unsupported("Кубик"),
        C::MessageStory(_) => unsupported("История"),
        C::MessageChecklist(_) => unsupported("Список задач"),
        C::MessageInvoice(_) => unsupported("Счёт"),
        C::MessageCall(m) => MessageContent::Unsupported(call_label(&m, author.is_outgoing)),
        C::MessageGroupCall(_) => unsupported("Групповой звонок"),
        C::MessageUnsupported => unsupported("Сообщение не поддерживается"),
        other => service::render(&other, author, names),
    }
}

fn labelled(label: &str, detail: &str) -> MessageContent {
    let detail = detail.trim();
    MessageContent::Unsupported(if detail.is_empty() {
        label.to_owned()
    } else {
        format!("{label}: {detail}")
    })
}

fn call_label(call: &types::MessageCall, is_outgoing: bool) -> String {
    let kind = if call.is_video {
        "видеозвонок"
    } else {
        "звонок"
    };
    match call.discard_reason {
        enums::CallDiscardReason::Missed if is_outgoing => format!("Отменённый {kind}"),
        enums::CallDiscardReason::Missed => format!("Пропущенный {kind}"),
        enums::CallDiscardReason::Declined => format!("Отклонённый {kind}"),
        _ => {
            let direction = if is_outgoing {
                "Исходящий"
            } else {
                "Входящий"
            };
            if call.duration > 0 {
                format!("{direction} {kind} ({})", ru::clock(call.duration))
            } else {
                format!("{direction} {kind}")
            }
        }
    }
}

/// The size to show inline: type "x" (800px) if present, else the largest one
/// that fits in 1280px, else the last one.
pub(crate) fn best_photo_size(sizes: &[types::PhotoSize]) -> Option<&types::PhotoSize> {
    sizes
        .iter()
        .find(|size| size.r#type == "x")
        .or_else(|| {
            sizes
                .iter()
                .filter(|size| size.width.max(size.height) <= 1280)
                .max_by_key(|size| i64::from(size.width) * i64::from(size.height))
        })
        .or_else(|| sizes.last())
}

pub(crate) fn join_name(first: &str, last: &str) -> String {
    match (first.trim(), last.trim()) {
        (first, "") => first.to_owned(),
        ("", last) => last.to_owned(),
        (first, last) => format!("{first} {last}"),
    }
}

/// One-line preview of message content for the chat list, e.g. "Фото, подпись".
pub(crate) fn preview_text(content: &MessageContent) -> String {
    let caption_of = |caption: &FormattedText| caption.text.clone();
    let (label, detail) = match content {
        MessageContent::Text(text) => return single_line(&text.text),
        MessageContent::Service(text) | MessageContent::Unsupported(text) => {
            return single_line(text);
        }
        MessageContent::Photo { caption, .. } => ("Фото", caption_of(caption)),
        MessageContent::Video { caption, .. } => ("Видео", caption_of(caption)),
        MessageContent::Animation { caption } => ("GIF", caption_of(caption)),
        MessageContent::Voice { caption, .. } => ("Голосовое сообщение", caption_of(caption)),
        MessageContent::VideoNote { .. } => ("Видеосообщение", String::new()),
        MessageContent::Document {
            file_name, caption, ..
        } => {
            let detail = if caption.text.trim().is_empty() {
                file_name.clone()
            } else {
                caption.text.clone()
            };
            ("Файл", detail)
        }
        MessageContent::Audio {
            title, performer, ..
        } => {
            let detail = match (performer.trim(), title.trim()) {
                ("", title) => title.to_owned(),
                (performer, "") => performer.to_owned(),
                (performer, title) => format!("{performer} — {title}"),
            };
            ("Аудио", detail)
        }
        MessageContent::Sticker { emoji } => {
            return single_line(&format!("Стикер {emoji}"));
        }
    };
    if detail.trim().is_empty() {
        label.to_owned()
    } else {
        single_line(&format!("{label}, {detail}"))
    }
}

/// Collapses whitespace (including newlines) and truncates to [`PREVIEW_CHARS`].
fn single_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(PREVIEW_CHARS * 4));
    for (i, word) in text.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(word);
        if out.len() >= PREVIEW_CHARS * 4 {
            break;
        }
    }
    match out.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => {
            out.truncate(cut);
            out.push('…');
            out
        }
        None => out,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A fixed name table for tests.
    #[derive(Default)]
    pub(crate) struct FakeNames {
        pub users: HashMap<UserId, (&'static str, &'static str)>,
        pub chats: HashMap<ChatId, &'static str>,
        pub channels: Vec<ChatId>,
    }

    impl Names for FakeNames {
        fn user_name(&self, user_id: UserId) -> Option<String> {
            self.users.get(&user_id).map(|(f, l)| join_name(f, l))
        }
        fn user_first_name(&self, user_id: UserId) -> Option<String> {
            self.users.get(&user_id).map(|(f, _)| f.to_string())
        }
        fn chat_title(&self, chat_id: ChatId) -> Option<String> {
            self.chats.get(&chat_id).map(|t| t.to_string())
        }
        fn is_channel(&self, chat_id: ChatId) -> bool {
            self.channels.contains(&chat_id)
        }
    }

    fn file(id: i32) -> types::File {
        types::File {
            id,
            size: 100,
            expected_size: 100,
            local: types::LocalFile {
                path: String::new(),
                can_be_downloaded: true,
                can_be_deleted: false,
                is_downloading_active: false,
                is_downloading_completed: false,
                download_offset: 0,
                downloaded_prefix_size: 0,
                downloaded_size: 0,
            },
            remote: types::RemoteFile {
                id: String::new(),
                unique_id: String::new(),
                is_uploading_active: false,
                is_uploading_completed: true,
                uploaded_size: 100,
            },
        }
    }

    fn size(kind: &str, width: i32, height: i32, id: i32) -> types::PhotoSize {
        types::PhotoSize {
            r#type: kind.into(),
            photo: file(id),
            width,
            height,
            progressive_sizes: vec![],
        }
    }

    #[test]
    fn photo_size_prefers_x_then_largest_fitting_then_last() {
        let sizes = vec![
            size("m", 320, 240, 1),
            size("x", 800, 600, 2),
            size("y", 1280, 960, 3),
        ];
        assert_eq!(best_photo_size(&sizes).unwrap().photo.id, 2);

        let sizes = vec![
            size("m", 320, 240, 1),
            size("y", 1280, 960, 3),
            size("w", 2560, 1920, 4),
        ];
        assert_eq!(best_photo_size(&sizes).unwrap().photo.id, 3);

        let sizes = vec![size("w", 2560, 1920, 4), size("z", 5000, 4000, 5)];
        assert_eq!(best_photo_size(&sizes).unwrap().photo.id, 5);
        assert!(best_photo_size(&[]).is_none());
    }

    fn caption(text: &str) -> FormattedText {
        plain(text.into())
    }

    #[test]
    fn previews_use_russian_labels() {
        let file = FileRefFixture::file();
        assert_eq!(
            preview_text(&MessageContent::Photo {
                file: file.clone(),
                width: 1,
                height: 1,
                caption: caption("закат\nна море"),
            }),
            "Фото, закат на море"
        );
        assert_eq!(
            preview_text(&MessageContent::Video {
                duration: 3,
                caption: caption("")
            }),
            "Видео"
        );
        assert_eq!(
            preview_text(&MessageContent::Document {
                file_name: "report.pdf".into(),
                file,
                caption: caption(""),
            }),
            "Файл, report.pdf"
        );
        assert_eq!(
            preview_text(&MessageContent::Voice {
                duration: 3,
                caption: caption("")
            }),
            "Голосовое сообщение"
        );
        assert_eq!(
            preview_text(&MessageContent::VideoNote { duration: 3 }),
            "Видеосообщение"
        );
        assert_eq!(
            preview_text(&MessageContent::Sticker {
                emoji: "😀".into()
            }),
            "Стикер 😀"
        );
        assert_eq!(
            preview_text(&MessageContent::Animation {
                caption: caption("")
            }),
            "GIF"
        );
        assert_eq!(
            preview_text(&MessageContent::Audio {
                title: "Song".into(),
                performer: "Band".into(),
                duration: 1
            }),
            "Аудио, Band — Song"
        );
        assert_eq!(
            preview_text(&MessageContent::Unsupported("Опрос: Когда?".into())),
            "Опрос: Когда?"
        );
    }

    #[test]
    fn long_previews_are_truncated() {
        let text = "слово ".repeat(100);
        let preview = preview_text(&MessageContent::Text(plain(text)));
        assert_eq!(preview.chars().count(), PREVIEW_CHARS + 1);
        assert!(preview.ends_with('…'));
    }

    struct FileRefFixture;
    impl FileRefFixture {
        fn file() -> crate::model::FileRef {
            file_ref(&file(1))
        }
    }

    pub(crate) fn tdlib_message(content: enums::MessageContent) -> types::Message {
        let json = serde_json::json!({
            "id": 10,
            "sender_id": {"@type": "messageSenderUser", "user_id": 1},
            "chat_id": -100,
            "is_outgoing": false,
            "is_pinned": false,
            "is_from_offline": false,
            "can_be_saved": true,
            "has_timestamped_media": false,
            "is_channel_post": false,
            "is_paid_star_suggested_post": false,
            "is_paid_ton_suggested_post": false,
            "contains_unread_mention": false,
            "date": 1700000000,
            "edit_date": 0,
            "unread_reactions": [],
            "reply_to": {"@type": "messageReplyToMessage", "chat_id": -100, "message_id": 5,
                         "checklist_task_id": 0, "origin_send_date": 0},
            "self_destruct_in": 0.0,
            "auto_delete_in": 0.0,
            "via_bot_user_id": 0,
            "sender_business_bot_user_id": 0,
            "sender_boost_count": 0,
            "paid_message_star_count": 0,
            "author_signature": "",
            "media_album_id": "0",
            "effect_id": "0",
            "summary_language_code": "",
            "content": serde_json::to_value(&content).unwrap(),
        });
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn full_message_conversion() {
        let mut names = FakeNames::default();
        names.users.insert(1, ("Анна", "Иванова"));
        names.users.insert(2, ("Борис", ""));
        let text = enums::MessageContent::MessageText(types::MessageText {
            text: types::FormattedText {
                text: "Привет".into(),
                entities: vec![],
            },
            link_preview: None,
            link_preview_options: None,
        });
        let converted = message(tdlib_message(text), &names);
        assert_eq!(converted.id, 10);
        assert_eq!(converted.sender, Sender::User(1));
        assert_eq!(converted.sender_name, "Анна Иванова");
        assert_eq!(converted.reply_to, Some(5));
        assert_eq!(converted.sending_state, SendingState::Sent);
        assert_eq!(
            converted.content,
            MessageContent::Text(plain("Привет".into()))
        );

        let added = enums::MessageContent::MessageChatAddMembers(types::MessageChatAddMembers {
            member_user_ids: vec![2],
        });
        let converted = message(tdlib_message(added), &names);
        assert_eq!(
            converted.content,
            MessageContent::Service("Анна Иванова добавил(а) Борис".into())
        );
    }
}
