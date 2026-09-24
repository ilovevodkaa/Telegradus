//! Message bubbles, media and rich text.

use chrono::NaiveDate;
use iced::font::{Style as FontStyle, Weight};
use iced::widget::text::Span;
use iced::widget::{
    Space, button, column, container, image, mouse_area, rich_text, row, sensor, span, stack, text,
};
use iced::{Alignment, Color, Element, Length, Padding, mouse};
use telegradus_core::{FileRef, MessageContent, MessageId, SendingState};

use crate::app::{App, Message};
use crate::format;
use crate::rich::{Block, Link, Segment};
use crate::state::history::{Item, OpenChat, ReplyPreview};
use crate::theme::{self, MONO, Palette, SANS, SANS_MEDIUM, SANS_SEMIBOLD};
use crate::ui::icons::{self, Icon, Tint};
use crate::ui::widgets;

/// Widest a bubble gets.
const BUBBLE_MAX_WIDTH: f32 = 480.0;
/// Photos fit into this square.
const PHOTO_MAX: f32 = 360.0;
const BODY_SIZE: f32 = 14.5;
/// Messages closer than this (seconds) from the same sender form one group.
const RUN_GAP: i64 = 5 * 60;

/// Shared inputs for rendering the messages of the open chat.
pub struct Context<'a> {
    pub app: &'a App,
    pub open: &'a OpenChat,
    pub is_group: bool,
    pub last_read: MessageId,
    pub palette: &'static Palette,
}

pub fn day_separator<'a>(day: NaiveDate, today: NaiveDate) -> Element<'a, Message> {
    container(
        container(text(format::day_label(day, today)).font(MONO).size(11))
            .padding(Padding::from([4.0, 12.0]))
            .style(theme::service_pill),
    )
    .center_x(Length::Fill)
    .padding(Padding::from([14.0, 0.0]))
    .into()
}

pub fn view<'a>(
    ctx: &Context<'a>,
    item: &'a Item,
    previous: Option<&Item>,
) -> Element<'a, Message> {
    let message = &item.message;
    if let MessageContent::Service(label) = &message.content {
        return sensor(
            container(
                container(text(label.as_str()).size(12.5))
                    .padding(Padding::from([5.0, 14.0]))
                    .style(theme::service_pill),
            )
            .center_x(Length::Fill)
            .padding(Padding::from([6.0, 0.0])),
        )
        .key(message.id)
        .into();
    }

    let first_in_run = previous.is_none_or(|p| {
        p.message.sender != message.sender
            || p.day != item.day
            || message.date - p.message.date > RUN_GAP
            || matches!(
                p.message.content,
                MessageContent::Service(_) | MessageContent::Sticker { .. }
            )
    });

    let content = match &message.content {
        MessageContent::Sticker { emoji } => sticker(ctx, item, emoji, first_in_run),
        _ => bubble(ctx, item, first_in_run),
    };

    let id = message.id;
    let reply_slot: Element<'a, Message> = if ctx.open.hovered == Some(id) {
        button(container(icons::icon(Icon::Reply, 15.0, Tint::Muted)).center(Length::Fill))
            .width(30)
            .height(30)
            .padding(0)
            .style(theme::button_icon)
            .on_press(Message::ReplyTo(id))
            .into()
    } else {
        Space::new().width(30).into()
    };
    let line = if message.is_outgoing {
        row![Space::new().width(Length::Fill), reply_slot, content]
    } else {
        row![content, reply_slot, Space::new().width(Length::Fill)]
    }
    .spacing(6)
    .align_y(Alignment::Center);

    let row = container(
        mouse_area(line)
            .on_enter(Message::Hover(id))
            .on_exit(Message::Unhover(id))
            .on_double_click(Message::ReplyTo(id)),
    )
    .padding(Padding::default().top(if first_in_run { 10.0 } else { 3.0 }));

    // Report unread incoming messages once they scroll into view. Every row
    // is a sensor keyed by message id, so positional widget state never
    // leaks between messages when older pages are prepended.
    let unread = !message.is_outgoing && id > ctx.last_read && !ctx.open.is_viewed(id);
    let row = sensor(row).key(id);
    if unread {
        row.on_show(move |_| Message::Seen(id)).into()
    } else {
        row.into()
    }
}

fn colors(p: &Palette, outgoing: bool) -> (Color, Color) {
    if outgoing {
        (p.on_accent, p.on_accent_muted)
    } else {
        (p.text, p.muted)
    }
}

fn bubble<'a>(ctx: &Context<'a>, item: &'a Item, first_in_run: bool) -> Element<'a, Message> {
    let message = &item.message;
    let outgoing = message.is_outgoing;
    let p = ctx.palette;
    let (fg, _) = colors(p, outgoing);
    let media_first = matches!(message.content, MessageContent::Photo { .. });

    // Text-like parts get their own padding when the bubble starts with a photo.
    let pad = |element: Element<'a, Message>| -> Element<'a, Message> {
        if media_first {
            container(element).padding(Padding::from([2.0, 8.0])).into()
        } else {
            element
        }
    };

    let mut parts: Vec<Element<'a, Message>> = Vec::new();
    if ctx.is_group && !outgoing && first_in_run {
        parts.push(pad(text(message.sender_name.as_str())
            .font(SANS_SEMIBOLD)
            .size(13)
            .color(fg)
            .into()));
    }
    if let Some(reply) = &item.reply {
        parts.push(pad(reply_quote(reply, outgoing, p)));
    } else if message.reply_to.is_some() {
        let generic = ReplyPreview {
            sender: "Ответ".into(),
            text: "сообщение не загружено".into(),
        };
        parts.push(pad(reply_quote_owned(generic, outgoing, p)));
    }
    if let Some(media) = media(ctx, item) {
        parts.push(media);
    }

    let body = body(ctx, item);
    let meta = meta(item, p);
    let bubble_content: Element<'a, Message> = match body {
        Some(body) if item.short && parts.is_empty() => {
            row![body, meta].spacing(10).align_y(Alignment::End).into()
        }
        Some(body) => {
            parts.push(pad(body));
            column![column(parts).spacing(5), pad(meta)]
                .align_x(Alignment::End)
                .into()
        }
        None => column![column(parts).spacing(5), pad(meta)]
            .spacing(4)
            .align_x(Alignment::End)
            .into(),
    };

    let padding = if media_first {
        Padding::from(3.0).bottom(6.0)
    } else {
        Padding::from([7.0, 12.0])
    };
    container(bubble_content)
        .padding(padding)
        .max_width(BUBBLE_MAX_WIDTH)
        .style(if outgoing {
            theme::bubble_out
        } else {
            theme::bubble_in
        })
        .into()
}

fn meta<'a>(item: &'a Item, p: &Palette) -> Element<'a, Message> {
    let message = &item.message;
    let outgoing = message.is_outgoing;
    let (_, muted) = colors(p, outgoing);
    let mut meta = row![].spacing(4).align_y(Alignment::Center);
    if message.edit_date > 0 {
        meta = meta.push(text("изм.").font(MONO).size(10.5).color(muted));
    }
    meta = meta.push(text(item.time.as_str()).font(MONO).size(10.5).color(muted));
    if outgoing {
        let tint = Tint::OnAccentMuted;
        meta = match &message.sending_state {
            SendingState::Sent => meta.push(icons::icon(Icon::Check, 12.0, tint)),
            SendingState::Pending => meta.push(icons::icon(Icon::Clock, 11.0, tint)),
            SendingState::Failed { error } => meta
                .push(
                    text(format!("не отправлено: {error}"))
                        .size(11)
                        .color(p.danger),
                )
                .push(icons::icon(Icon::Alert, 12.0, Tint::Danger)),
        };
    }
    meta.into()
}

fn sticker<'a>(
    ctx: &Context<'a>,
    item: &'a Item,
    emoji: &'a str,
    first_in_run: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let outgoing = item.message.is_outgoing;
    let sender = (ctx.is_group && !outgoing && first_in_run).then(|| {
        text(item.message.sender_name.as_str())
            .font(SANS_SEMIBOLD)
            .size(13)
    });
    let meta = container(
        row![
            text(item.time.as_str())
                .font(MONO)
                .size(10.5)
                .color(p.muted)
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([2.0, 8.0]))
    .style(theme::service_pill);
    column![sender, text(emoji).font(theme::EMOJI).size(72), meta]
        .spacing(2)
        .align_x(if outgoing {
            Alignment::End
        } else {
            Alignment::Start
        })
        .into()
}

fn quote_frame<'a>(content: Element<'a, Message>, outgoing: bool) -> Element<'a, Message> {
    let bar = container(Space::new())
        .width(3)
        .height(Length::Fill)
        .style(theme::quote_bar(outgoing));
    stack![
        container(content)
            .padding(Padding {
                top: 4.0,
                right: 10.0,
                bottom: 4.0,
                left: 11.0,
            })
            .style(theme::reply_quote(outgoing)),
        bar,
    ]
    .into()
}

fn reply_quote<'a>(reply: &'a ReplyPreview, outgoing: bool, p: &Palette) -> Element<'a, Message> {
    let (fg, muted) = colors(p, outgoing);
    quote_frame(
        column![
            text(reply.sender.as_str())
                .font(SANS_SEMIBOLD)
                .size(12.5)
                .color(fg),
            widgets::line(&reply.text, SANS, 12.5, muted),
        ]
        .into(),
        outgoing,
    )
}

fn reply_quote_owned<'a>(reply: ReplyPreview, outgoing: bool, p: &Palette) -> Element<'a, Message> {
    let (fg, muted) = colors(p, outgoing);
    quote_frame(
        column![
            text(reply.sender).font(SANS_SEMIBOLD).size(12.5).color(fg),
            text(reply.text).size(12.5).color(muted),
        ]
        .into(),
        outgoing,
    )
}

fn media<'a>(ctx: &Context<'a>, item: &'a Item) -> Option<Element<'a, Message>> {
    let outgoing = item.message.is_outgoing;
    let p = ctx.palette;
    let (fg, muted) = colors(p, outgoing);
    let element = match &item.message.content {
        MessageContent::Photo {
            file,
            width,
            height,
            ..
        } => photo(ctx, file, *width, *height, outgoing),
        MessageContent::Document {
            file_name, file, ..
        } => document(ctx, file, file_name, outgoing),
        MessageContent::Video { duration, .. } => labeled(
            Icon::Play,
            "Видео",
            format::duration(*duration),
            outgoing,
            p,
        ),
        MessageContent::Animation { .. } => {
            labeled(Icon::Play, "GIF-анимация", String::new(), outgoing, p)
        }
        MessageContent::VideoNote { duration } => labeled(
            Icon::Play,
            "Видеосообщение",
            format::duration(*duration),
            outgoing,
            p,
        ),
        MessageContent::Audio {
            title,
            performer,
            duration,
        } => {
            let name = format::content_preview(&item.message.content).into_owned();
            let _ = (title, performer);
            labeled(Icon::Play, name, format::duration(*duration), outgoing, p)
        }
        MessageContent::Voice { duration, .. } => voice(item.message.id, *duration, outgoing, p),
        MessageContent::Unsupported(label) => text(label.as_str())
            .size(BODY_SIZE)
            .font(theme::SANS_ITALIC)
            .color(muted)
            .into(),
        MessageContent::Text(_) | MessageContent::Sticker { .. } | MessageContent::Service(_) => {
            return None;
        }
    };
    let _ = fg;
    Some(element)
}

fn photo<'a>(
    ctx: &Context<'a>,
    file: &'a FileRef,
    width: i32,
    height: i32,
    outgoing: bool,
) -> Element<'a, Message> {
    let (w, h) = format::media_size(width, height, PHOTO_MAX);
    let id = file.id;
    let files = &ctx.app.files;
    if let Some(handle) = files.image(id) {
        let mask = if outgoing {
            Tint::Accent
        } else {
            Tint::SurfaceAlt
        };
        return mouse_area(stack![
            container(
                image(handle.clone())
                    .width(w)
                    .height(h)
                    .content_fit(iced::ContentFit::Cover)
            )
            .padding(1),
            icons::corner_mask(12.0, mask),
        ])
        .interaction(mouse::Interaction::Pointer)
        .on_press(Message::OpenFile(id))
        .into();
    }
    let state = files.get(id).unwrap_or(file);
    let label = if state.is_downloading || files.is_requested(id) {
        progress_label(state)
    } else {
        "фото".to_owned()
    };
    let placeholder = container(
        column![
            icons::icon(Icon::Download, 18.0, Tint::Muted),
            text(label).font(MONO).size(11),
        ]
        .spacing(6)
        .align_x(Alignment::Center),
    )
    .center_x(w + 2.0)
    .center_y(h + 2.0)
    .style(theme::media_placeholder);
    if files.is_requested(id) || state.is_downloaded() {
        placeholder.into()
    } else {
        sensor(placeholder)
            .key(id)
            .on_show(move |_| Message::MediaShown(id))
            .into()
    }
}

fn progress_label(file: &FileRef) -> String {
    if file.size > 0 && file.downloaded_size > 0 {
        format!(
            "{}%",
            (file.downloaded_size * 100 / file.size).clamp(0, 100)
        )
    } else {
        "загрузка…".to_owned()
    }
}

fn round_icon<'a>(icon: Icon, outgoing: bool, size: f32) -> Element<'a, Message> {
    let tint = if outgoing {
        Tint::Accent
    } else {
        Tint::OnAccent
    };
    container(icons::icon(icon, size * 0.42, tint))
        .center_x(size)
        .center_y(size)
        .style(move |theme| {
            let p = theme::palette(theme);
            container::Style {
                background: Some(if outgoing { p.on_accent } else { p.accent }.into()),
                border: iced::border::rounded(999),
                ..container::Style::default()
            }
        })
        .into()
}

fn document<'a>(
    ctx: &Context<'a>,
    file: &'a FileRef,
    name: &'a str,
    outgoing: bool,
) -> Element<'a, Message> {
    let p = ctx.palette;
    let (fg, muted) = colors(p, outgoing);
    let files = &ctx.app.files;
    let id = file.id;
    let state = files.get(id).unwrap_or(file);
    let size = format::file_size(file.size);
    let (icon, detail, on_press) = if state.is_downloaded() {
        (
            Icon::File,
            format!("{size} · открыть"),
            Some(Message::OpenFile(id)),
        )
    } else if state.is_downloading || files.is_requested(id) {
        let done = format::file_size(state.downloaded_size);
        (Icon::Download, format!("{done} из {size}"), None)
    } else {
        (
            Icon::Download,
            format!("{size} · скачать"),
            Some(Message::Download(id)),
        )
    };
    let content = row![
        round_icon(icon, outgoing, 42.0),
        column![
            text(format::truncate_to_width(name, 260.0, 14.0).into_owned())
                .font(SANS_MEDIUM)
                .size(14)
                .color(fg)
                .wrapping(text::Wrapping::None),
            text(detail).font(MONO).size(11).color(muted),
        ]
        .spacing(2),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    button(content)
        .padding(Padding::from([2.0, 0.0]))
        .style(move |_, _| button::Style {
            background: None,
            text_color: fg,
            ..button::Style::default()
        })
        .on_press_maybe(on_press)
        .into()
}

fn labeled<'a>(
    icon: Icon,
    label: impl text::IntoFragment<'a>,
    detail: String,
    outgoing: bool,
    p: &Palette,
) -> Element<'a, Message> {
    let (fg, muted) = colors(p, outgoing);
    row![
        round_icon(icon, outgoing, 38.0),
        column![
            text(label).font(SANS_MEDIUM).size(14).color(fg),
            text(detail).font(MONO).size(11).color(muted),
        ]
        .spacing(2),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn voice<'a>(id: MessageId, duration: i32, outgoing: bool, p: &Palette) -> Element<'a, Message> {
    let (_, muted) = colors(p, outgoing);
    // A deterministic pseudo-waveform: TDLib's real waveform is not in the model.
    let bars = (0..28u64).map(|i| {
        let mut x =
            (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ i.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x ^= x >> 29;
        let height = 4.0 + (x % 15) as f32;
        container(Space::new())
            .width(3)
            .height(height)
            .style(move |_| container::Style {
                background: Some(muted.into()),
                border: iced::border::rounded(2),
                ..container::Style::default()
            })
            .into()
    });
    row![
        round_icon(Icon::Play, outgoing, 38.0),
        column![
            row(bars).spacing(2).align_y(Alignment::Center).height(20),
            text(format::duration(duration))
                .font(MONO)
                .size(11)
                .color(muted),
        ]
        .spacing(3),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn body<'a>(ctx: &Context<'a>, item: &'a Item) -> Option<Element<'a, Message>> {
    if item.blocks.is_empty() {
        return None;
    }
    let outgoing = item.message.is_outgoing;
    let revealed = ctx.open.is_revealed(item.message.id);
    let source = item.text();
    let p = ctx.palette;
    let blocks = item.blocks.iter().map(|block| -> Element<'a, Message> {
        match block {
            Block::Text(segments) => {
                paragraph(source, segments, item.message.id, outgoing, revealed, p)
            }
            Block::Quote(segments) => quote_frame(
                paragraph(source, segments, item.message.id, outgoing, revealed, p),
                outgoing,
            ),
            Block::Pre { range, language } => {
                let code = text(source.get(range.clone()).unwrap_or_default())
                    .font(MONO)
                    .size(12.5)
                    .line_height(1.45);
                let content: Element<'a, Message> = if language.is_empty() {
                    code.into()
                } else {
                    column![
                        text(language.as_str())
                            .font(MONO)
                            .size(10.5)
                            .color(colors(p, outgoing).1),
                        code
                    ]
                    .spacing(4)
                    .into()
                };
                container(content)
                    .padding(Padding::from([8.0, 10.0]))
                    .style(theme::code_block(outgoing))
                    .into()
            }
        }
    });
    Some(column(blocks).spacing(6).into())
}

/// A run of styled segments as rich text.
fn paragraph<'a>(
    source: &'a str,
    segments: &'a [Segment],
    message_id: MessageId,
    outgoing: bool,
    revealed: bool,
    p: &Palette,
) -> Element<'a, Message> {
    let (fg, muted) = colors(p, outgoing);
    let spans: Vec<Span<'a, Link>> = segments
        .iter()
        .map(|segment| {
            let style = segment.style;
            let content = source.get(segment.range.clone()).unwrap_or_default();
            let mut font = if style.code {
                MONO
            } else if style.emoji {
                theme::EMOJI
            } else {
                SANS
            };
            if !style.code && !style.emoji {
                if style.bold {
                    font.weight = Weight::Semibold;
                } else if style.accent {
                    font.weight = Weight::Medium;
                }
                if style.italic {
                    font.style = FontStyle::Italic;
                }
            }
            let mut span = span(content)
                .font(font)
                .underline(style.underline)
                .strikethrough(style.strikethrough)
                .link_maybe(segment.link.clone());
            if style.code {
                span = span
                    .size(BODY_SIZE - 1.5)
                    .background(fg.scale_alpha(0.1))
                    .border(iced::border::rounded(4))
                    .padding(Padding::from([0.0, 2.0]));
            }
            if style.spoiler && !revealed {
                span = span
                    .color(muted)
                    .background(muted)
                    .border(iced::border::rounded(3));
            }
            span
        })
        .collect();
    rich_text(spans)
        .on_link_click(move |link| Message::Link(message_id, link))
        .size(BODY_SIZE)
        .line_height(1.4)
        .color(fg)
        .into()
}
