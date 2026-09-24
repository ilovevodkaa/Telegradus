//! The chat pane: header, message history and composer.

use iced::keyboard::{self, key};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text_editor::{self, Binding, KeyPress};
use iced::widget::{Column, Space, button, column, container, row, scrollable, stack, text};
use iced::{Alignment, Element, Length, Padding};
use telegradus_core::{ChatKind, ConnectionState};

use crate::app::{App, COMPOSER_ID, HISTORY_ID, Message};
use crate::format;
use crate::state::history::OpenChat;
use crate::theme::{self, SANS_MEDIUM, SANS_SEMIBOLD};
use crate::ui::icons::{self, Icon, Tint};
use crate::ui::message;
use crate::ui::widgets::{self, mono};

pub fn view(app: &App) -> Element<'_, Message> {
    let Some(open) = &app.open else {
        return empty();
    };
    column![
        header(app, open),
        rule(),
        history(app, open),
        composer(app, open)
    ]
    .into()
}

fn rule<'a>() -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(theme::line)
        .into()
}

fn empty<'a>() -> Element<'a, Message> {
    let hint = stack![
        container(
            mono(
                "Enter — отправить  ·  Shift+Enter — новая строка  ·  Esc — закрыть",
                11.0
            )
            .style(theme::text_muted)
        )
        .padding(Padding::from([7.0, 16.0])),
        icons::dashed_pill(Tint::LineStrong),
    ];
    container(
        column![
            text("Выберите чат, чтобы начать переписку")
                .font(SANS_MEDIUM)
                .size(16),
            text("Сообщения загружаются только для открытого чата — так клиент остаётся лёгким.")
                .size(13.5)
                .style(theme::text_muted),
            Space::new().height(6),
            hint,
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center(Length::Fill)
    .padding(32)
    .into()
}

fn subtitle(app: &App, kind: ChatKind) -> String {
    if app.connection != ConnectionState::Ready {
        return match app.connection {
            ConnectionState::WaitingForNetwork => "ожидание сети…".to_owned(),
            ConnectionState::Updating => "обновление…".to_owned(),
            _ => "соединение…".to_owned(),
        };
    }
    match kind {
        ChatKind::Private { user_id } if app.me.as_ref().is_some_and(|me| me.id == user_id) => {
            "заметки для себя".to_owned()
        }
        ChatKind::Private { .. } => "личный чат".to_owned(),
        ChatKind::Secret { .. } => "секретный чат".to_owned(),
        ChatKind::BasicGroup | ChatKind::Supergroup => "группа".to_owned(),
        ChatKind::Channel => "канал".to_owned(),
    }
}

fn header<'a>(app: &'a App, open: &'a OpenChat) -> Element<'a, Message> {
    let Some(chat) = app.chats.get(open.id) else {
        return container(Space::new()).height(60).into();
    };
    let summary = &chat.summary;
    let info = column![
        widgets::line(
            &format::truncate_to_width(&summary.title, 520.0, 15.0),
            SANS_SEMIBOLD,
            15.0,
            app.palette().text,
        ),
        mono(subtitle(app, summary.kind), 11.0).style(theme::text_muted),
    ]
    .spacing(2);
    container(
        row![
            widgets::avatar(chat, app.avatar(chat), 38.0, app.is_dark()),
            info,
            Space::new().width(Length::Fill),
            widgets::icon_button(Icon::Close, Tint::Muted, 32.0, Some(Message::CloseChat)),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .height(60)
    .padding(Padding::from([0.0, 20.0]))
    .center_y(60)
    .into()
}

fn history<'a>(app: &'a App, open: &'a OpenChat) -> Element<'a, Message> {
    if !open.loaded {
        return container(mono("загрузка…", 11.0).style(theme::text_muted))
            .center(Length::Fill)
            .into();
    }
    if open.items.is_empty() {
        return container(text("Сообщений пока нет").size(14).style(theme::text_muted))
            .center(Length::Fill)
            .into();
    }
    let chat = app.chats.get(open.id).map(|c| &c.summary);
    let is_group =
        chat.is_some_and(|c| matches!(c.kind, ChatKind::BasicGroup | ChatKind::Supergroup));
    let last_read = chat.map_or(i64::MAX, |c| c.last_read_inbox_message_id);
    let ctx = message::Context {
        app,
        open,
        is_group,
        last_read,
        palette: app.palette(),
    };

    let top: Element<'a, Message> = if open.loading_older || open.has_more_older {
        mono("загрузка…", 11.0).style(theme::text_muted).into()
    } else {
        mono("начало истории", 11.0).style(theme::text_muted).into()
    };
    // A plain column: iced's keyed column mis-diffs insertions at the ends.
    let mut list = Column::with_capacity(open.items.len() + 8)
        .width(Length::Fill)
        .push(
            container(top)
                .center_x(Length::Fill)
                .padding(Padding::from([12.0, 0.0])),
        );
    let mut previous: Option<&crate::state::history::Item> = None;
    for item in &open.items {
        if previous.is_none_or(|p| p.day != item.day) {
            list = list.push(message::day_separator(item.day, app.today));
        }
        list = list.push(message::view(&ctx, item, previous));
        previous = Some(item);
    }
    list = list.push(Space::new().height(14));

    let content = container(list)
        .padding(Padding::from([0.0, 20.0]))
        .max_width(900);
    let scroll = scrollable(container(content).center_x(Length::Fill))
        .id(HISTORY_ID)
        // `direction` resets the anchor, so it must come first.
        .direction(Direction::Vertical(
            Scrollbar::new().width(5).scroller_width(5).margin(2),
        ))
        .anchor_bottom()
        .on_scroll(Message::HistoryScrolled)
        .height(Length::Shrink)
        .style(theme::scrollbar);
    // Short histories stick to the bottom like in every messenger.
    container(scroll)
        .height(Length::Fill)
        .align_bottom(Length::Fill)
        .into()
}

fn composer<'a>(app: &'a App, open: &'a OpenChat) -> Element<'a, Message> {
    let summary = app.chats.get(open.id).map(|c| &c.summary);
    let can_send = summary.is_some_and(|s| s.can_send_messages);
    if !can_send {
        let notice = if summary.is_some_and(|s| s.kind == ChatKind::Channel) {
            "Публиковать в этом канале могут только администраторы"
        } else {
            "Вы не можете отправлять сообщения в этот чат"
        };
        return container(
            container(text(notice).size(13.5))
                .padding(Padding::from([12.0, 18.0]))
                .center_x(Length::Fill)
                .style(theme::notice),
        )
        .padding(Padding::from([12.0, 20.0]))
        .into();
    }

    let editor = iced::widget::text_editor(&app.composer)
        .id(COMPOSER_ID)
        .placeholder("Сообщение…")
        .on_action(Message::Composer)
        .key_binding(bindings)
        .padding(Padding::from([10.0, 16.0]))
        .size(14.5)
        .max_height(180.0)
        .style(theme::editor);

    let has_text = !app.composer.is_empty();
    let send =
        button(container(icons::icon(Icon::Send, 18.0, Tint::OnAccent)).center(Length::Fill))
            .width(42)
            .height(42)
            .padding(0)
            .style(theme::button_primary)
            .on_press_maybe(has_text.then_some(Message::Send));

    // The chip slot is always present (empty when not replying) so the
    // editor keeps its place in the widget tree, and with it the focus.
    let chip = app.reply_to.map(|reply_id| {
        container(reply_chip(open, reply_id)).padding(Padding::default().bottom(8))
    });
    let content = column![chip, row![editor, send].spacing(10).align_y(Alignment::End)];
    container(content)
        .padding(Padding {
            top: 8.0,
            right: 20.0,
            bottom: 16.0,
            left: 20.0,
        })
        .max_width(940)
        .center_x(Length::Fill)
        .into()
}

fn bindings(press: KeyPress) -> Option<Binding<Message>> {
    let focused = matches!(press.status, text_editor::Status::Focused { .. });
    match press.key.as_ref() {
        keyboard::Key::Named(key::Named::Enter) if focused && !press.modifiers.shift() => {
            Some(Binding::Custom(Message::Send))
        }
        keyboard::Key::Named(key::Named::Escape) if focused => {
            Some(Binding::Custom(Message::Escape))
        }
        _ => Binding::from_key_press(press),
    }
}

fn reply_chip(open: &OpenChat, reply_id: i64) -> Element<'_, Message> {
    let (sender, preview) = match open.find(reply_id) {
        Some(item) => (
            if item.message.is_outgoing {
                "Вы".to_owned()
            } else {
                item.message.sender_name.clone()
            },
            format::truncate_to_width(&format::content_preview(&item.message.content), 560.0, 13.0)
                .into_owned(),
        ),
        None => ("Ответ".to_owned(), "сообщение".to_owned()),
    };
    let bar = container(Space::new())
        .width(3)
        .height(32)
        .style(theme::quote_bar(false));
    container(
        row![
            icons::icon(Icon::Reply, 16.0, Tint::Muted),
            bar,
            column![
                text(sender).font(SANS_SEMIBOLD).size(12.5),
                text(preview)
                    .size(13)
                    .style(theme::text_muted)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(1)
            .width(Length::Fill),
            widgets::icon_button(Icon::Close, Tint::Muted, 28.0, Some(Message::CancelReply)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8.0, 12.0]))
    .clip(true)
    .style(theme::composer_chip)
    .into()
}
