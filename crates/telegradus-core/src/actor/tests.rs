//! Actor behaviour without a TDLib client: updates are injected directly.

use std::path::PathBuf;

use tdlib_rs::enums::{self, Update};
use tdlib_rs::types;
use tokio::sync::mpsc;

use super::{Actor, IncomingUpdate};
use crate::history::OpenChat;
use crate::message::tests::tdlib_message;
use crate::model::{ChatListId, MessageContent};
use crate::{Command, Config, Event};

struct Harness {
    actor: Actor,
    updates: mpsc::UnboundedSender<IncomingUpdate>,
    events: mpsc::UnboundedReceiver<Event>,
    _commands: mpsc::UnboundedSender<Command>,
}

impl Harness {
    fn new() -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (update_tx, update_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let config = Config {
            api: None,
            data_dir: PathBuf::from("unused"),
            use_test_dc: false,
        };
        Self {
            // Client id 0 matches the injected updates.
            actor: Actor::new(config, command_rx, update_rx, event_tx),
            updates: update_tx,
            events: event_rx,
            _commands: command_tx,
        }
    }

    fn push(&self, update: Update) {
        self.updates.send((Box::new(update), 0)).unwrap();
    }

    /// Processes everything queued and flushes, returning the emitted events.
    fn process(&mut self) -> Vec<Event> {
        self.actor.drain();
        self.actor.flush();
        std::iter::from_fn(|| self.events.try_recv().ok()).collect()
    }
}

fn chat(id: i64, order: i64) -> types::Chat {
    types::Chat {
        id,
        r#type: enums::ChatType::BasicGroup(types::ChatTypeBasicGroup { basic_group_id: id }),
        title: format!("Чат {id}"),
        photo: None,
        accent_color_id: 0,
        background_custom_emoji_id: 0,
        upgraded_gift_colors: None,
        profile_accent_color_id: 0,
        profile_background_custom_emoji_id: 0,
        permissions: types::ChatPermissions {
            can_send_basic_messages: true,
            ..Default::default()
        },
        last_message: None,
        positions: vec![types::ChatPosition {
            list: enums::ChatList::Main,
            order,
            is_pinned: false,
            source: None,
        }],
        chat_lists: vec![enums::ChatList::Main],
        message_sender_id: None,
        block_list: None,
        has_protected_content: false,
        is_translatable: false,
        is_marked_as_unread: false,
        view_as_topics: false,
        has_scheduled_messages: false,
        can_be_deleted_only_for_self: false,
        can_be_deleted_for_all_users: false,
        can_be_reported: false,
        default_disable_notification: false,
        unread_count: 0,
        last_read_inbox_message_id: 0,
        last_read_outbox_message_id: 0,
        unread_mention_count: 0,
        unread_reaction_count: 0,
        notification_settings: Default::default(),
        available_reactions: enums::ChatAvailableReactions::All(Default::default()),
        message_auto_delete_time: 0,
        emoji_status: None,
        background: None,
        theme: None,
        action_bar: None,
        business_bot_manage_bar: None,
        video_chat: Default::default(),
        pending_join_requests: None,
        reply_markup_message_id: 0,
        draft_message: None,
        client_data: String::new(),
    }
}

fn text_message(chat_id: i64, id: i64, text: &str) -> types::Message {
    let mut message = tdlib_message(enums::MessageContent::MessageText(types::MessageText {
        text: types::FormattedText {
            text: text.into(),
            entities: vec![],
        },
        link_preview: None,
        link_preview_options: None,
    }));
    message.chat_id = chat_id;
    message.id = id;
    message.reply_to = None;
    message
}

#[test]
fn a_burst_of_updates_becomes_one_batch() {
    let mut h = Harness::new();
    for id in 1..=1000 {
        h.push(Update::NewChat(types::UpdateNewChat { chat: chat(id, id) }));
    }
    // Moves chat 1 to the top and renames it within the same burst.
    h.push(Update::ChatPosition(types::UpdateChatPosition {
        chat_id: 1,
        position: types::ChatPosition {
            list: enums::ChatList::Main,
            order: 5000,
            is_pinned: true,
            source: None,
        },
    }));
    h.push(Update::ChatTitle(types::UpdateChatTitle {
        chat_id: 1,
        title: "Первый".into(),
    }));

    let events = h.process();
    assert_eq!(events.len(), 2, "{events:?}");
    let Event::ChatsUpdated(chats) = &events[0] else {
        panic!("expected ChatsUpdated, got {:?}", events[0]);
    };
    assert_eq!(chats.len(), 1000);
    let first = chats.iter().find(|c| c.id == 1).unwrap();
    assert_eq!(first.title, "Первый");
    assert!(first.can_send_messages);
    let Event::ChatListUpdated {
        list,
        entries,
        has_more,
    } = &events[1]
    else {
        panic!("expected ChatListUpdated, got {:?}", events[1]);
    };
    assert_eq!(*list, ChatListId::Main);
    assert!(*has_more);
    assert_eq!(entries.len(), 1000);
    assert_eq!((entries[0].chat_id, entries[0].is_pinned), (1, true));
    assert_eq!(entries[1].chat_id, 1000);
    assert_eq!(entries[999].chat_id, 2);

    // Nothing changed: nothing is sent.
    assert!(h.process().is_empty());

    // An update that changes neither the summary nor the order is swallowed.
    h.push(Update::ChatTitle(types::UpdateChatTitle {
        chat_id: 1,
        title: "Первый".into(),
    }));
    assert!(h.process().is_empty());
}

#[test]
fn fully_loaded_lists_are_reported() {
    let mut h = Harness::new();
    let not_found = types::Error {
        code: 404,
        message: "Not Found".into(),
    };
    h.actor.on_chats_loaded(ChatListId::Archive, Err(not_found));
    let events = h.process();
    assert!(
        matches!(
            &events[..],
            [Event::ChatListUpdated {
                list: ChatListId::Archive,
                has_more: false,
                ..
            }]
        ),
        "{events:?}"
    );

    // Other errors are reported, and the list is still sent so the UI stops waiting.
    let error = types::Error {
        code: 500,
        message: "Request aborted".into(),
    };
    h.actor.on_chats_loaded(ChatListId::Main, Err(error));
    let events = h.process();
    assert!(matches!(&events[0], Event::Error(e) if e.message == "Запрос прерван."));
    assert!(matches!(
        &events[1],
        Event::ChatListUpdated {
            list: ChatListId::Main,
            has_more: true,
            ..
        }
    ));
}

#[test]
fn message_updates_of_open_chats() {
    let mut h = Harness::new();
    h.push(Update::NewChat(types::UpdateNewChat {
        chat: chat(-100, 1),
    }));
    h.process();
    h.actor.open_chats.insert(-100, OpenChat::default());

    // New messages of closed chats are ignored.
    h.push(Update::NewMessage(types::UpdateNewMessage {
        message: text_message(-5, 1, "мимо"),
    }));
    h.push(Update::NewMessage(types::UpdateNewMessage {
        message: text_message(-100, 7, "привет"),
    }));
    let events = h.process();
    assert!(
        matches!(&events[..], [Event::MessageAdded(m)] if m.id == 7),
        "{events:?}"
    );

    h.push(Update::MessageEdited(types::UpdateMessageEdited {
        chat_id: -100,
        message_id: 7,
        edit_date: 1234,
        reply_markup: None,
    }));
    h.push(Update::MessageContent(types::UpdateMessageContent {
        chat_id: -100,
        message_id: 7,
        new_content: enums::MessageContent::MessageText(types::MessageText {
            text: types::FormattedText {
                text: "привет!".into(),
                entities: vec![],
            },
            link_preview: None,
            link_preview_options: None,
        }),
    }));
    let events = h.process();
    assert!(matches!(&events[0], Event::MessageUpdated(m) if m.edit_date == 1234));
    assert!(matches!(
        &events[1],
        Event::MessageUpdated(m) if m.content == MessageContent::Text(crate::text::plain("привет!".into()))
    ));

    h.push(Update::MessageSendSucceeded(
        types::UpdateMessageSendSucceeded {
            message: text_message(-100, 8, "отправлено"),
            old_message_id: 7,
        },
    ));
    let events = h.process();
    assert!(
        matches!(&events[..], [Event::MessageSent { old_id: 7, message }] if message.id == 8),
        "{events:?}"
    );

    // Only permanent deletions are reported.
    for is_permanent in [false, true] {
        h.push(Update::DeleteMessages(types::UpdateDeleteMessages {
            chat_id: -100,
            message_ids: vec![8],
            is_permanent,
            from_cache: !is_permanent,
        }));
    }
    let events = h.process();
    assert!(
        matches!(&events[..], [Event::MessagesDeleted { chat_id: -100, message_ids }] if message_ids == &[8]),
        "{events:?}"
    );
    assert!(h.actor.open_chats[&-100].get(8).is_none());
}

#[test]
fn updates_of_other_clients_are_ignored() {
    let mut h = Harness::new();
    h.updates
        .send((
            Box::new(Update::NewChat(types::UpdateNewChat { chat: chat(1, 1) })),
            42,
        ))
        .unwrap();
    assert!(h.process().is_empty());
}

#[test]
fn rejected_credentials_reopen_the_setup_form_before_the_error() {
    let mut h = Harness::new();
    h.actor.on_auth_result(Err(types::Error {
        code: 400,
        message: "API_ID_INVALID".to_owned(),
    }));
    let events = h.process();
    // The UI switches to the setup form first, then shows the error on it.
    assert!(
        matches!(
            &events[..],
            [Event::Auth(crate::AuthState::NeedApiCredentials), Event::Error(error)]
                if error.context == crate::ErrorContext::Auth
        ),
        "{events:?}"
    );

    // Other login errors leave the current step as it is.
    h.actor.on_auth_result(Err(types::Error {
        code: 400,
        message: "PHONE_CODE_INVALID".to_owned(),
    }));
    let events = h.process();
    assert!(matches!(&events[..], [Event::Error(_)]), "{events:?}");
}
