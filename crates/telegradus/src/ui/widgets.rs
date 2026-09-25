//! Small reusable pieces: wordmark, avatars, badges, labels.

use iced::widget::text::Span;
use iced::widget::{Space, button, container, image, rich_text, row, span, stack, text};
use iced::{Alignment, Color, Element, Font, Length, Never, Padding, never};
use telegradus_core::ConnectionState;

use crate::app::Message;
use crate::state::chats::Chat;
use crate::theme::{self, MONO, SANS_SEMIBOLD};
use crate::ui::icons::{self, Tint};

/// The `_telegradus` wordmark: an ink underscore bar followed by the name.
pub fn wordmark<'a>(size: f32) -> Element<'a, Message> {
    let bar = container(Space::new())
        .width(size * 0.5)
        .height((size * 0.13).round().max(2.0))
        .style(|theme| container::Style {
            background: Some(theme::palette(theme).text.into()),
            ..container::Style::default()
        });
    row![
        container(bar).padding(Padding::default().bottom(size * 0.2)),
        text("telegradus").font(SANS_SEMIBOLD).size(size),
    ]
    .spacing(size * 0.08)
    .align_y(Alignment::End)
    .into()
}

/// A colored dot and a short label for the TDLib connection state.
pub fn connection<'a>(state: ConnectionState) -> Element<'a, Message> {
    let (label, color): (&str, fn(&theme::Palette) -> iced::Color) = match state {
        ConnectionState::Ready => ("в сети", |p| p.success),
        ConnectionState::Updating => ("обновление…", |p| p.warning),
        ConnectionState::Connecting | ConnectionState::ConnectingToProxy => {
            ("соединение…", |p| p.warning)
        }
        ConnectionState::WaitingForNetwork => ("нет сети", |p| p.danger),
    };
    let dot = container(Space::new().width(7).height(7)).style(move |theme| container::Style {
        background: Some(color(theme::palette(theme)).into()),
        border: iced::border::rounded(999),
        ..container::Style::default()
    });
    row![dot, mono(label, 11.0).style(theme::text_muted)]
        .spacing(7)
        .align_y(Alignment::Center)
        .into()
}

/// Appends `content` as spans, giving emoji runs the color emoji font.
pub fn push_spans<'a>(spans: &mut Vec<Span<'a, Never>>, content: &str, font: Font, color: Color) {
    for (range, emoji) in crate::rich::emoji_runs(content, 0..content.len()) {
        let font = if emoji { theme::EMOJI } else { font };
        spans.push(span(content[range].to_owned()).font(font).color(color));
    }
}

/// A single line of text (no wrapping) whose emoji use the color emoji font.
pub fn line<'a>(content: &str, font: Font, size: f32, color: Color) -> Element<'a, Message> {
    if !content.chars().any(crate::rich::is_emoji) {
        return text(content.to_owned())
            .font(font)
            .size(size)
            .color(color)
            .wrapping(text::Wrapping::None)
            .into();
    }
    let mut spans = Vec::new();
    push_spans(&mut spans, content, font, color);
    rich_text(spans)
        .on_link_click(never)
        .size(size)
        .wrapping(text::Wrapping::None)
        .into()
}

/// Small monospace meta text.
pub fn mono<'a>(content: impl text::IntoFragment<'a>, size: f32) -> text::Text<'a> {
    text(content).font(MONO).size(size)
}

/// Round avatar: a prepared thumbnail, or initials on a grey circle.
pub fn avatar<'a>(
    chat: &'a Chat,
    thumbnail: Option<&image::Handle>,
    size: f32,
    dark: bool,
) -> Element<'a, Message> {
    initials_avatar(chat.initials.as_str(), chat.shade, thumbnail, size, dark)
}

pub fn initials_avatar<'a>(
    initials: impl text::IntoFragment<'a>,
    shade: usize,
    thumbnail: Option<&image::Handle>,
    size: f32,
    dark: bool,
) -> Element<'a, Message> {
    if let Some(handle) = thumbnail {
        return image(handle.clone()).width(size).height(size).into();
    }
    let (background, foreground) = theme::avatar_colors(shade, dark);
    container(
        text(initials)
            .font(SANS_SEMIBOLD)
            .size((size * 0.36).round())
            .color(foreground),
    )
    .center_x(size)
    .center_y(size)
    .style(theme::avatar(background, foreground))
    .into()
}

/// Unread counter pill.
pub fn badge<'a>(count: i32, muted: bool, selected: bool) -> Element<'a, Message> {
    let label = if count > 999 {
        "999+".to_owned()
    } else {
        count.to_string()
    };
    container(text(label).font(SANS_SEMIBOLD).size(11.5))
        .padding(Padding::from([1.0, 6.5]))
        .height(20)
        .center_y(20)
        .style(theme::badge(muted, selected))
        .into()
}

/// The "@" unread-mention marker.
pub fn mention_badge<'a>(selected: bool) -> Element<'a, Message> {
    container(text("@").font(SANS_SEMIBOLD).size(12))
        .center_x(20)
        .center_y(20)
        .style(theme::badge(false, selected))
        .into()
}

/// A chip with a dashed outline (inactive) or a filled ink pill (active).
pub fn chip<'a>(
    label: Element<'a, Message>,
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let padding = Padding::from([5.0, 12.0]);
    if active {
        button(label)
            .padding(padding)
            .style(theme::button_primary)
            .on_press(on_press)
            .into()
    } else {
        stack![
            button(label)
                .padding(padding)
                .style(theme::button_chip)
                .on_press(on_press),
            icons::dashed_pill(Tint::LineStrong),
        ]
        .into()
    }
}

/// Round icon button.
pub fn icon_button<'a>(
    icon: icons::Icon,
    tint: Tint,
    size: f32,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(container(icons::icon(icon, size * 0.45, tint)).center(Length::Fill))
        .width(size)
        .height(size)
        .padding(0)
        .style(theme::button_icon)
        .on_press_maybe(on_press)
        .into()
}
