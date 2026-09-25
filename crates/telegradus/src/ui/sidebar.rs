//! The left sidebar: wordmark, connection status, folder tabs, chat list.

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
    Column, Space, button, column, container, rich_text, row, scrollable, span, text,
};
use iced::{Alignment, Element, Length, Padding, never};
use telegradus_core::{ChatListEntry, ChatListId};

use crate::app::{App, CHAT_LIST_ID, CHAT_ROW_HEIGHT, Message};
use crate::format;
use crate::state::chats::Chat;
use crate::theme::{self, MONO, SANS, SANS_MEDIUM, SANS_SEMIBOLD};
use crate::ui::icons::{self, Icon, Tint};
use crate::ui::widgets::{self, connection, mono, wordmark};
use crate::virtual_list;

/// Rows built beyond the viewport on each side.
const OVERSCAN: usize = 4;
/// Horizontal room for the text column of a row.
const ROW_TEXT_WIDTH: f32 = super::SIDEBAR_WIDTH - 16.0 - 20.0 - 48.0 - 12.0;
const AVATAR_SIZE: f32 = 48.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let header = row![
        wordmark(19.0),
        Space::new().width(Length::Fill),
        widgets::icon_button(Icon::Theme, Tint::Muted, 30.0, Some(Message::ToggleTheme)),
    ]
    .align_y(Alignment::Center);

    column![
        container(column![header, connection(app.connection)].spacing(6)).padding(Padding {
            top: 18.0,
            right: 14.0,
            bottom: 12.0,
            left: 20.0,
        }),
        folders(app),
        chat_list(app),
        footer(app),
    ]
    .into()
}

fn folders(app: &App) -> Element<'_, Message> {
    let selected = app.selected_list;
    let label = |title: &str, list: ChatListId| -> Element<'_, Message> {
        let unread = app.chats.unread_chats(list);
        let title = text(title.to_owned()).font(SANS_MEDIUM).size(13);
        if unread > 0 && list != selected {
            row![
                title,
                mono(unread.to_string(), 10.5).style(theme::text_muted)
            ]
            .spacing(5)
            .align_y(Alignment::Center)
            .into()
        } else {
            title.into()
        }
    };
    let mut tabs = vec![widgets::chip(
        label("Все чаты", ChatListId::Main),
        selected == ChatListId::Main,
        Message::SelectList(ChatListId::Main),
    )];
    for folder in app.chats.folders() {
        let list = ChatListId::Folder(folder.id);
        tabs.push(widgets::chip(
            label(&folder.title, list),
            selected == list,
            Message::SelectList(list),
        ));
    }
    tabs.push(widgets::chip(
        row![
            icons::icon(
                Icon::Archive,
                13.0,
                if selected == ChatListId::Archive {
                    Tint::OnAccent
                } else {
                    Tint::Text
                }
            ),
            text("Архив").font(SANS_MEDIUM).size(13),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        selected == ChatListId::Archive,
        Message::SelectList(ChatListId::Archive),
    ));
    // Chips wrap onto more lines instead of hiding behind a horizontal scroll.
    container(row(tabs).spacing(6).wrap().vertical_spacing(6))
        .padding(Padding {
            top: 0.0,
            right: 16.0,
            bottom: 12.0,
            left: 16.0,
        })
        .into()
}

fn chat_list(app: &App) -> Element<'_, Message> {
    let Some(list) = app.chats.list(app.selected_list) else {
        return placeholder("загрузка…");
    };
    if list.entries.is_empty() {
        return placeholder(if list.has_more || list.loading {
            "загрузка…"
        } else {
            "здесь пока пусто"
        });
    }
    let entries: &[ChatListEntry] = &list.entries;
    let viewport = app.list_viewport;
    let window = virtual_list::window(
        entries.len(),
        CHAT_ROW_HEIGHT,
        viewport.offset,
        viewport.height,
        OVERSCAN,
    );

    // A plain column: iced's keyed column mis-diffs insertions at the ends.
    let mut rows = Column::with_capacity(window.rows.len() + 3)
        .width(Length::Fill)
        .push(Space::new().height(window.space_before));
    for entry in &entries[window.rows.clone()] {
        let row: Element<'_, Message> = match app.chats.get(entry.chat_id) {
            Some(chat) => chat_row(app, chat, entry.is_pinned),
            // Keep the row's widget type so its state stays aligned.
            None => button(Space::new())
                .width(Length::Fill)
                .height(CHAT_ROW_HEIGHT)
                .style(theme::chat_row(false))
                .into(),
        };
        rows = rows.push(row);
    }
    let tail = if list.has_more {
        "загрузка…"
    } else {
        ""
    };
    rows = rows.push(Space::new().height(window.space_after)).push(
        container(mono(tail, 11.0).style(theme::text_muted))
            .center_x(Length::Fill)
            .padding(Padding::from([10.0, 0.0])),
    );

    scrollable(container(rows).padding(Padding::from([0.0, 8.0])))
        .id(CHAT_LIST_ID)
        .direction(Direction::Vertical(
            Scrollbar::new().width(4).scroller_width(4).margin(2),
        ))
        .on_scroll(Message::ChatListScrolled)
        .height(Length::Fill)
        .style(theme::scrollbar)
        .into()
}

fn placeholder(label: &str) -> Element<'_, Message> {
    container(mono(label, 11.0).style(theme::text_muted))
        .center_x(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::default().top(40))
        .into()
}

fn chat_row<'a>(app: &'a App, chat: &'a Chat, is_pinned: bool) -> Element<'a, Message> {
    let summary = &chat.summary;
    let selected = app.open.as_ref().is_some_and(|open| open.id == summary.id);
    let p = app.palette();
    let (primary, secondary) = if selected {
        (p.on_accent, p.on_accent_muted)
    } else {
        (p.text, p.muted)
    };

    // First line: title, mute mark, time.
    let time = summary
        .last_message
        .as_ref()
        .map(|m| format::list_time(format::local_datetime(m.date), app.today))
        .unwrap_or_default();
    let time_width = time.chars().count() as f32 * 6.8 + 8.0;
    let mute_width = if summary.is_muted { 18.0 } else { 0.0 };
    let title = format::truncate_to_width(
        &summary.title,
        ROW_TEXT_WIDTH - time_width - mute_width,
        14.5,
    );
    let mut first = row![widgets::line(&title, SANS_SEMIBOLD, 14.5, primary)]
        .spacing(5)
        .align_y(Alignment::Center);
    if summary.is_muted {
        first = first.push(icons::icon(
            Icon::Muted,
            12.0,
            if selected {
                Tint::OnAccentMuted
            } else {
                Tint::Muted
            },
        ));
    }
    let first = row![
        first,
        Space::new().width(Length::Fill),
        text(time).font(MONO).size(11).color(secondary),
    ]
    .align_y(Alignment::Center);

    // Second line: preview and badges.
    let mut badges = row![].spacing(5).align_y(Alignment::Center);
    let mut badges_width = 0.0;
    if summary.unread_mention_count > 0 {
        badges = badges.push(widgets::mention_badge(selected));
        badges_width += 25.0;
    }
    if summary.unread_count > 0 {
        badges = badges.push(widgets::badge(
            summary.unread_count,
            summary.is_muted,
            selected,
        ));
        badges_width += summary.unread_count.min(9999).to_string().len() as f32 * 7.5 + 18.0;
    } else if is_pinned {
        badges = badges.push(icons::icon(
            Icon::Pin,
            13.0,
            if selected {
                Tint::OnAccentMuted
            } else {
                Tint::Muted
            },
        ));
        badges_width += 18.0;
    }
    let preview_width = ROW_TEXT_WIDTH - badges_width - 6.0;
    let preview = preview(
        summary,
        preview_width,
        primary,
        secondary,
        p.danger,
        selected,
    );

    let content = row![
        widgets::avatar(chat, app.avatar(chat), AVATAR_SIZE, app.is_dark()),
        column![
            first,
            row![preview, Space::new().width(Length::Fill), badges].align_y(Alignment::Center),
        ]
        .spacing(4)
        .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    button(container(content).center_y(Length::Fill).clip(true))
        .width(Length::Fill)
        .height(CHAT_ROW_HEIGHT)
        .padding(Padding::from([0.0, 10.0]))
        .style(theme::chat_row(selected))
        .on_press(Message::OpenChat(summary.id))
        .into()
}

fn footer(app: &App) -> Element<'_, Message> {
    let Some(me) = &app.me else {
        return Space::new().into();
    };
    let name = format!("{} {}", me.first_name, me.last_name)
        .trim()
        .to_owned();
    let handle = match &me.username {
        Some(username) => format!("@{username}"),
        None => me.phone_number.clone(),
    };
    let actions: Element<'_, Message> = if app.logout_armed {
        row![
            button(text("Выйти").font(SANS_MEDIUM).size(12.5))
                .padding(Padding::from([5.0, 11.0]))
                .style(theme::button_primary)
                .on_press(Message::LogOut),
            button(text("Отмена").font(SANS_MEDIUM).size(12.5))
                .padding(Padding::from([5.0, 11.0]))
                .style(theme::button_ghost)
                .on_press(Message::CancelLogOut),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    } else {
        widgets::icon_button(Icon::Logout, Tint::Muted, 30.0, Some(Message::AskLogOut))
    };
    let content = row![
        widgets::initials_avatar(
            format::initials(&name),
            format::shade_index(me.id),
            None,
            34.0,
            app.is_dark()
        ),
        column![
            text(format::truncate_to_width(&name, 150.0, 13.5).into_owned())
                .font(SANS_SEMIBOLD)
                .size(13.5)
                .wrapping(text::Wrapping::None),
            mono(handle, 11.0)
                .style(theme::text_muted)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(1),
        Space::new().width(Length::Fill),
        actions,
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    column![
        container(Space::new())
            .width(Length::Fill)
            .height(1)
            .style(theme::line),
        container(content).padding(Padding {
            top: 10.0,
            right: 12.0,
            bottom: 12.0,
            left: 16.0,
        }),
    ]
    .into()
}

fn preview<'a>(
    summary: &'a telegradus_core::ChatSummary,
    width: f32,
    primary: iced::Color,
    secondary: iced::Color,
    danger: iced::Color,
    selected: bool,
) -> Element<'a, Message> {
    let size = 13.5;
    let (label, label_color, body) = match summary.draft.as_deref().filter(|d| !d.is_empty()) {
        Some(draft) => (
            Some("Черновик"),
            if selected { primary } else { danger },
            format::one_line(draft),
        ),
        None => {
            let Some(last) = &summary.last_message else {
                return Space::new().into();
            };
            let (prefix, body) = format::preview_parts(last);
            (prefix, primary, body)
        }
    };
    let Some(label) = label else {
        return widgets::line(
            &format::truncate_to_width(&body, width, size),
            SANS,
            size,
            secondary,
        );
    };
    let label_width = label.chars().count() as f32 * size * 0.55 + size * 0.6;
    let body = format::truncate_to_width(&body, width - label_width, size);
    let mut spans = vec![
        span(format!("{label}: "))
            .font(SANS_MEDIUM)
            .color(label_color),
    ];
    widgets::push_spans(&mut spans, &body, SANS, secondary);
    rich_text(spans)
        .on_link_click(never)
        .size(size)
        .wrapping(text::Wrapping::None)
        .into()
}
