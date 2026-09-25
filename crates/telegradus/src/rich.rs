//! Turns TDLib-style formatted text (text + nested entities) into flat,
//! styled segments that the view maps to `rich_text` spans.
//!
//! The work happens once per message when it arrives; the view only borrows
//! string slices, so re-rendering is allocation-light.

use std::ops::Range;
use std::sync::Arc;

use telegradus_core::{FormattedText, TextEntity, TextEntityKind};

use crate::format::normalize_url;

/// Inline style flags of a segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub spoiler: bool,
    /// Mentions, hashtags, commands: drawn with a heavier weight.
    pub accent: bool,
    /// Emoji run, drawn with the color emoji font.
    pub emoji: bool,
}

/// What a click on a segment does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Url(Arc<str>),
    /// Reveal the spoilers of the message.
    Spoiler,
}

/// A run of text with uniform style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Byte range into the source text.
    pub range: Range<usize>,
    pub style: Style,
    pub link: Option<Link>,
}

/// A paragraph-level piece of a message body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text(Vec<Segment>),
    /// Preformatted code block (`range` into the source text).
    Pre {
        range: Range<usize>,
        language: String,
    },
    Quote(Vec<Segment>),
}

/// Splits formatted text into blocks of styled segments.
pub fn blocks(formatted: &FormattedText) -> Vec<Block> {
    let text = formatted.text.as_str();
    let entities: Vec<&TextEntity> = formatted
        .entities
        .iter()
        .filter(|e| is_valid_range(text, &e.range))
        .collect();

    let mut out = Vec::new();
    let mut cursor = 0;
    let mut index = 0;
    while index < entities.len() {
        let entity = entities[index];
        let is_block = matches!(
            entity.kind,
            TextEntityKind::Pre { .. } | TextEntityKind::Blockquote
        );
        if !is_block || entity.range.start < cursor {
            index += 1;
            continue;
        }
        // Text before the block, without the newline that separates it.
        let before = trim_newlines(text, cursor..entity.range.start, cursor > 0, true);
        push_text(&mut out, text, before, &entities);
        match &entity.kind {
            TextEntityKind::Pre { language } => {
                let range = trim_newlines(text, entity.range.clone(), true, true);
                if !range.is_empty() {
                    out.push(Block::Pre {
                        range,
                        language: language.clone(),
                    });
                }
            }
            _ => {
                let range = trim_newlines(text, entity.range.clone(), true, true);
                let inner: Vec<&TextEntity> = entities
                    .iter()
                    .copied()
                    .filter(|e| !std::ptr::eq(*e, entity))
                    .collect();
                let segments = segments(text, range, &inner);
                if !segments.is_empty() {
                    out.push(Block::Quote(segments));
                }
            }
        }
        cursor = entity.range.end;
        index += 1;
    }
    let rest = trim_newlines(text, cursor..text.len(), cursor > 0, false);
    push_text(&mut out, text, rest, &entities);
    out
}

fn push_text(out: &mut Vec<Block>, text: &str, range: Range<usize>, entities: &[&TextEntity]) {
    if range.is_empty() {
        return;
    }
    let segments = segments(text, range, entities);
    if !segments.is_empty() {
        out.push(Block::Text(segments));
    }
}

fn is_valid_range(text: &str, range: &Range<usize>) -> bool {
    range.start < range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}

/// Drops one leading and/or trailing newline from `range`.
fn trim_newlines(
    text: &str,
    mut range: Range<usize>,
    leading: bool,
    trailing: bool,
) -> Range<usize> {
    let bytes = text.as_bytes();
    if leading && range.start < range.end && bytes[range.start] == b'\n' {
        range.start += 1;
    }
    if trailing && range.start < range.end && bytes[range.end - 1] == b'\n' {
        range.end -= 1;
    }
    range
}

/// Flattens the entities covering `range` into non-overlapping segments.
pub fn segments(text: &str, range: Range<usize>, entities: &[&TextEntity]) -> Vec<Segment> {
    let inside: Vec<&TextEntity> = entities
        .iter()
        .copied()
        .filter(|e| e.range.start < range.end && e.range.end > range.start)
        .collect();

    let mut bounds = Vec::with_capacity(inside.len() * 2 + 2);
    bounds.push(range.start);
    bounds.push(range.end);
    for e in &inside {
        bounds.push(e.range.start.clamp(range.start, range.end));
        bounds.push(e.range.end.clamp(range.start, range.end));
    }
    bounds.sort_unstable();
    bounds.dedup();

    let mut out: Vec<Segment> = Vec::with_capacity(bounds.len());
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let mut style = Style::default();
        let mut link: Option<(usize, Link)> = None;
        for e in inside
            .iter()
            .filter(|e| e.range.start <= start && e.range.end >= end)
        {
            if let Some(candidate) = apply(&mut style, e, text) {
                let len = e.range.len();
                if link.as_ref().is_none_or(|(best, _)| len < *best) {
                    link = Some((len, candidate));
                }
            }
        }
        let link = if style.spoiler {
            Some(Link::Spoiler)
        } else {
            link.map(|(_, link)| link)
        };
        // Emoji get their own runs so they can use the color emoji font.
        for (run, emoji) in emoji_runs(text, start..end) {
            let style = Style { emoji, ..style };
            match out.last_mut() {
                Some(last)
                    if last.style == style && last.link == link && last.range.end == run.start =>
                {
                    last.range.end = run.end;
                }
                _ => out.push(Segment {
                    range: run,
                    style,
                    link: link.clone(),
                }),
            }
        }
    }
    out
}

/// Whether `c` starts or continues an emoji sequence.
pub fn is_emoji(c: char) -> bool {
    matches!(
        c as u32,
        0x1F000..=0x1FAFF   // pictographs, emoticons, transport, flags, skin tones
            | 0x2600..=0x27BF   // misc symbols, dingbats
            | 0x2B00..=0x2BFF   // arrows and stars (⭐, ⬛)
            | 0x231A..=0x231B
            | 0x23E9..=0x23FA
            | 0x200D            // zero-width joiner
            | 0xFE0F            // emoji presentation selector
            | 0x20E3            // keycap
            | 0xE0020..=0xE007F // tag sequences
    )
}

/// Splits `range` of `text` into alternating non-emoji and emoji runs.
pub fn emoji_runs(text: &str, range: Range<usize>) -> Vec<(Range<usize>, bool)> {
    let mut runs: Vec<(Range<usize>, bool)> = Vec::new();
    for (offset, c) in text[range.clone()].char_indices() {
        let start = range.start + offset;
        let end = start + c.len_utf8();
        let emoji = is_emoji(c);
        match runs.last_mut() {
            Some((run, kind)) if *kind == emoji => run.end = end,
            _ => runs.push((start..end, emoji)),
        }
    }
    runs
}

/// Applies an entity to a style and returns the link it carries, if any.
fn apply(style: &mut Style, entity: &TextEntity, text: &str) -> Option<Link> {
    let url = |s: String| Some(Link::Url(Arc::from(s)));
    match &entity.kind {
        TextEntityKind::Bold => style.bold = true,
        TextEntityKind::Italic => style.italic = true,
        TextEntityKind::Underline => style.underline = true,
        TextEntityKind::Strikethrough => style.strikethrough = true,
        TextEntityKind::Spoiler => style.spoiler = true,
        TextEntityKind::Code | TextEntityKind::Pre { .. } => style.code = true,
        TextEntityKind::Blockquote | TextEntityKind::Other => {}
        TextEntityKind::Url => {
            style.underline = true;
            return url(normalize_url(&text[entity.range.clone()]));
        }
        TextEntityKind::TextUrl { url: target } => {
            style.underline = true;
            return url(normalize_url(target));
        }
        TextEntityKind::Email => {
            style.underline = true;
            return url(format!("mailto:{}", &text[entity.range.clone()]));
        }
        TextEntityKind::Mention => {
            style.accent = true;
            let name = text[entity.range.clone()].trim_start_matches('@');
            return url(format!("https://t.me/{name}"));
        }
        TextEntityKind::PhoneNumber => {
            style.underline = true;
            let digits: String = text[entity.range.clone()]
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '+')
                .collect();
            return url(format!("tel:{digits}"));
        }
        TextEntityKind::MentionName { .. }
        | TextEntityKind::Hashtag
        | TextEntityKind::BotCommand => {
            style.accent = true;
        }
    }
    None
}

/// Whether the message body is a single short line (drawn inline with its time).
pub fn is_short(text: &str, blocks: &[Block]) -> bool {
    matches!(blocks, [Block::Text(_)]) && text.chars().count() <= 36 && !text.contains('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(range: Range<usize>, kind: TextEntityKind) -> TextEntity {
        TextEntity { range, kind }
    }

    fn formatted(text: &str, entities: Vec<TextEntity>) -> FormattedText {
        FormattedText {
            text: text.to_owned(),
            entities,
        }
    }

    fn text_segments(block: &Block) -> &[Segment] {
        match block {
            Block::Text(s) | Block::Quote(s) => s,
            Block::Pre { .. } => panic!("expected text"),
        }
    }

    #[test]
    fn plain_text_is_one_segment() {
        let blocks = blocks(&formatted("привет", vec![]));
        assert_eq!(blocks.len(), 1);
        let segs = text_segments(&blocks[0]);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].range, 0..12);
        assert_eq!(segs[0].style, Style::default());
    }

    #[test]
    fn nested_bold_inside_link() {
        // "see [docs **here**]" — link covers "docs here", bold covers "here".
        let text = "see docs here now";
        let ft = formatted(
            text,
            vec![
                entity(
                    4..13,
                    TextEntityKind::TextUrl {
                        url: "https://docs.rs".into(),
                    },
                ),
                entity(9..13, TextEntityKind::Bold),
            ],
        );
        let blocks = blocks(&ft);
        let segs = text_segments(&blocks[0]);
        let ranges: Vec<_> = segs.iter().map(|s| s.range.clone()).collect();
        assert_eq!(ranges, vec![0..4, 4..9, 9..13, 13..17]);
        let link = Some(Link::Url(Arc::from("https://docs.rs")));
        assert_eq!(segs[1].link, link);
        assert!(segs[1].style.underline && !segs[1].style.bold);
        assert_eq!(segs[2].link, link);
        assert!(segs[2].style.underline && segs[2].style.bold);
        assert_eq!(segs[3].link, None);
    }

    #[test]
    fn innermost_link_wins() {
        let text = "a b.com c";
        let ft = formatted(
            text,
            vec![
                entity(
                    0..9,
                    TextEntityKind::TextUrl {
                        url: "x.org".into(),
                    },
                ),
                entity(2..7, TextEntityKind::Url),
            ],
        );
        let blocks = blocks(&ft);
        let segs = text_segments(&blocks[0]);
        assert_eq!(segs[0].link, Some(Link::Url(Arc::from("https://x.org"))));
        assert_eq!(segs[1].link, Some(Link::Url(Arc::from("https://b.com"))));
    }

    #[test]
    fn adjacent_equal_styles_merge() {
        let text = "abcdef";
        let ft = formatted(
            text,
            vec![
                entity(0..3, TextEntityKind::Bold),
                entity(3..6, TextEntityKind::Bold),
            ],
        );
        let blocks = blocks(&ft);
        let segs = text_segments(&blocks[0]);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].range, 0..6);
        assert!(segs[0].style.bold);
    }

    #[test]
    fn pre_becomes_a_block_and_trims_separators() {
        let text = "Смотри:\nfn main() {}\nГотово";
        let start = "Смотри:\n".len();
        let end = start + "fn main() {}".len();
        let ft = formatted(
            text,
            vec![entity(
                start..end,
                TextEntityKind::Pre {
                    language: "rust".into(),
                },
            )],
        );
        let blocks = blocks(&ft);
        assert_eq!(blocks.len(), 3);
        assert_eq!(text_segments(&blocks[0])[0].range, 0..start - 1);
        assert_eq!(
            blocks[1],
            Block::Pre {
                range: start..end,
                language: "rust".into()
            }
        );
        assert_eq!(text_segments(&blocks[2])[0].range, end + 1..text.len());
    }

    #[test]
    fn quote_keeps_inner_formatting() {
        let text = "цитата жирно";
        let ft = formatted(
            text,
            vec![
                entity(0..text.len(), TextEntityKind::Blockquote),
                entity("цитата ".len()..text.len(), TextEntityKind::Bold),
            ],
        );
        let blocks = blocks(&ft);
        assert_eq!(blocks.len(), 1);
        let Block::Quote(segs) = &blocks[0] else {
            panic!("expected quote")
        };
        assert_eq!(segs.len(), 2);
        assert!(segs[1].style.bold);
    }

    #[test]
    fn invalid_ranges_are_ignored() {
        let text = "ж"; // two bytes
        let ft = formatted(
            text,
            vec![
                entity(0..1, TextEntityKind::Bold),
                entity(0..10, TextEntityKind::Italic),
            ],
        );
        let blocks = blocks(&ft);
        let segs = text_segments(&blocks[0]);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].style, Style::default());
    }

    #[test]
    fn spoiler_and_mentions() {
        let text = "@durov secret";
        let ft = formatted(
            text,
            vec![
                entity(0..6, TextEntityKind::Mention),
                entity(7..13, TextEntityKind::Spoiler),
            ],
        );
        let blocks = blocks(&ft);
        let segs = text_segments(&blocks[0]);
        assert_eq!(
            segs[0].link,
            Some(Link::Url(Arc::from("https://t.me/durov")))
        );
        assert!(segs[0].style.accent);
        assert_eq!(segs[2].link, Some(Link::Spoiler));
    }

    #[test]
    fn emoji_get_their_own_runs() {
        let text = "ок 👍🏽 да ❤️";
        let blocks = blocks(&formatted(
            text,
            vec![entity(0..text.len(), TextEntityKind::Bold)],
        ));
        let segs = text_segments(&blocks[0]);
        let runs: Vec<(&str, bool, bool)> = segs
            .iter()
            .map(|s| (&text[s.range.clone()], s.style.emoji, s.style.bold))
            .collect();
        assert_eq!(
            runs,
            vec![
                ("ок ", false, true),
                ("👍🏽", true, true),
                (" да ", false, true),
                ("❤️", true, true),
            ]
        );
    }

    #[test]
    fn short_detection() {
        let ft = formatted("Ок, созвонимся", vec![]);
        assert!(is_short(&ft.text, &blocks(&ft)));
        let ft = formatted("строка\nещё", vec![]);
        assert!(!is_short(&ft.text, &blocks(&ft)));
    }
}
