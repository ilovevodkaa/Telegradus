//! TDLib formatted text to [`FormattedText`] conversion.
//!
//! TDLib measures entity offsets and lengths in UTF-16 code units; the model
//! uses UTF-8 byte ranges that always lie on char boundaries.

use tdlib_rs::{enums::TextEntityType, types};

use crate::model::{FormattedText, TextEntity, TextEntityKind};

/// Converts TDLib formatted text, dropping entities that are empty, invalid or
/// partially overlap an earlier entity.
pub(crate) fn formatted_text(text: types::FormattedText) -> FormattedText {
    let types::FormattedText { text, entities } = text;
    if entities.is_empty() {
        return plain(text);
    }

    // Every UTF-16 position we need, sorted, so a single pass over the text maps them all.
    let mut offsets: Vec<usize> = entities
        .iter()
        .filter_map(|e| utf16_span(e.offset, e.length))
        .flat_map(|(start, end)| [start, end])
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    let bytes = utf16_to_utf8(&text, &offsets);
    let byte_at = |utf16: usize| {
        offsets
            .binary_search(&utf16)
            .ok()
            .and_then(|i| bytes.get(i).copied())
    };

    let mut converted: Vec<TextEntity> = entities
        .into_iter()
        .filter_map(|entity| {
            let (start, end) = utf16_span(entity.offset, entity.length)?;
            let range = byte_at(start)?..byte_at(end)?;
            (!range.is_empty()).then(|| TextEntity {
                range,
                kind: entity_kind(entity.r#type),
            })
        })
        .collect();
    // Outer entities first when two start at the same position.
    converted.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then(b.range.end.cmp(&a.range.end))
    });

    FormattedText {
        text,
        entities: drop_partial_overlaps(converted),
    }
}

/// Text without any formatting.
pub(crate) fn plain(text: String) -> FormattedText {
    FormattedText {
        text,
        entities: Vec::new(),
    }
}

/// `(start, end)` in UTF-16 units, or `None` for negative or empty spans.
fn utf16_span(offset: i32, length: i32) -> Option<(usize, usize)> {
    let start = usize::try_from(offset).ok()?;
    let length = usize::try_from(length).ok().filter(|l| *l > 0)?;
    Some((start, start.saturating_add(length)))
}

/// Maps sorted UTF-16 offsets to UTF-8 byte offsets. Offsets past the end are
/// clamped to the text length; an offset inside a surrogate pair is moved to
/// the end of that character.
fn utf16_to_utf8(text: &str, sorted_offsets: &[usize]) -> Vec<usize> {
    let mut result = Vec::with_capacity(sorted_offsets.len());
    let mut chars = text.char_indices();
    let mut utf16 = 0;
    let mut byte = 0;
    for &target in sorted_offsets {
        while utf16 < target {
            let Some((index, ch)) = chars.next() else {
                break;
            };
            utf16 += ch.len_utf16();
            byte = index + ch.len_utf8();
        }
        result.push(byte);
    }
    result
}

/// Keeps entities that nest properly; `entities` must be sorted by start, outer first.
fn drop_partial_overlaps(entities: Vec<TextEntity>) -> Vec<TextEntity> {
    let mut kept: Vec<TextEntity> = Vec::with_capacity(entities.len());
    // Ends of the currently open (enclosing) entities.
    let mut open_ends: Vec<usize> = Vec::new();
    for entity in entities {
        while open_ends
            .last()
            .is_some_and(|end| *end <= entity.range.start)
        {
            open_ends.pop();
        }
        if open_ends.last().is_some_and(|end| *end < entity.range.end) {
            continue;
        }
        open_ends.push(entity.range.end);
        kept.push(entity);
    }
    kept
}

fn entity_kind(kind: TextEntityType) -> TextEntityKind {
    match kind {
        TextEntityType::Bold => TextEntityKind::Bold,
        TextEntityType::Italic => TextEntityKind::Italic,
        TextEntityType::Underline => TextEntityKind::Underline,
        TextEntityType::Strikethrough => TextEntityKind::Strikethrough,
        TextEntityType::Spoiler => TextEntityKind::Spoiler,
        TextEntityType::Code => TextEntityKind::Code,
        TextEntityType::Pre => TextEntityKind::Pre {
            language: String::new(),
        },
        TextEntityType::PreCode(pre) => TextEntityKind::Pre {
            language: pre.language,
        },
        TextEntityType::BlockQuote | TextEntityType::ExpandableBlockQuote => {
            TextEntityKind::Blockquote
        }
        TextEntityType::Url => TextEntityKind::Url,
        TextEntityType::TextUrl(link) => TextEntityKind::TextUrl { url: link.url },
        TextEntityType::EmailAddress => TextEntityKind::Email,
        TextEntityType::Mention => TextEntityKind::Mention,
        TextEntityType::MentionName(mention) => TextEntityKind::MentionName {
            user_id: mention.user_id,
        },
        TextEntityType::Hashtag | TextEntityType::Cashtag => TextEntityKind::Hashtag,
        TextEntityType::BotCommand => TextEntityKind::BotCommand,
        TextEntityType::PhoneNumber => TextEntityKind::PhoneNumber,
        TextEntityType::BankCardNumber
        | TextEntityType::CustomEmoji(_)
        | TextEntityType::MediaTimestamp(_) => TextEntityKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(offset: i32, length: i32, kind: TextEntityType) -> types::TextEntity {
        types::TextEntity {
            offset,
            length,
            r#type: kind,
        }
    }

    fn convert(text: &str, entities: Vec<types::TextEntity>) -> FormattedText {
        formatted_text(types::FormattedText {
            text: text.to_owned(),
            entities,
        })
    }

    fn slices(ft: &FormattedText) -> Vec<&str> {
        ft.entities
            .iter()
            .map(|e| &ft.text[e.range.clone()])
            .collect()
    }

    #[test]
    fn plain_text_has_no_entities() {
        let ft = convert("hello", vec![]);
        assert_eq!(ft, plain("hello".into()));
    }

    #[test]
    fn cyrillic_offsets_are_converted_to_bytes() {
        // "мир" starts at UTF-16 offset 7 and byte offset 13.
        let ft = convert("Привет мир", vec![entity(7, 3, TextEntityType::Bold)]);
        assert_eq!(ft.entities[0].range, 13..19);
        assert_eq!(slices(&ft), ["мир"]);
    }

    #[test]
    fn surrogate_pairs_count_as_two_units() {
        let text = "a😀b 👍🏻 end";
        let ft = convert(
            text,
            vec![
                entity(1, 2, TextEntityType::Italic),
                entity(3, 1, TextEntityType::Bold),
                entity(5, 4, TextEntityType::Underline),
            ],
        );
        assert_eq!(slices(&ft), ["😀", "b", "👍🏻"]);
    }

    #[test]
    fn offsets_inside_a_surrogate_pair_snap_to_char_boundaries() {
        // Starts in the middle of the emoji: the entity would be empty and is dropped.
        let ft = convert("a😀b", vec![entity(2, 1, TextEntityType::Bold)]);
        assert!(ft.entities.is_empty());
        // Ends in the middle of the emoji: extended to cover it.
        let ft = convert("a😀b", vec![entity(0, 2, TextEntityType::Bold)]);
        assert_eq!(slices(&ft), ["a😀"]);
        for e in &ft.entities {
            assert!(ft.text.is_char_boundary(e.range.start));
            assert!(ft.text.is_char_boundary(e.range.end));
        }
    }

    #[test]
    fn nested_entities_are_sorted_outer_first() {
        let url = TextEntityType::TextUrl(types::TextEntityTypeTextUrl {
            url: "https://example.org".into(),
        });
        let ft = convert(
            "жирная ссылка",
            vec![entity(7, 6, TextEntityType::Bold), entity(0, 13, url)],
        );
        assert_eq!(ft.entities.len(), 2);
        assert!(matches!(
            ft.entities[0].kind,
            TextEntityKind::TextUrl { .. }
        ));
        assert_eq!(slices(&ft), ["жирная ссылка", "ссылка"]);
    }

    #[test]
    fn invalid_and_overlapping_entities_are_dropped_or_clamped() {
        let ft = convert(
            "0123456789",
            vec![
                entity(-1, 3, TextEntityType::Bold),
                entity(2, 0, TextEntityType::Bold),
                entity(0, 5, TextEntityType::Italic),
                entity(3, 4, TextEntityType::Code),
                entity(8, 100, TextEntityType::Underline),
                entity(50, 2, TextEntityType::Strikethrough),
            ],
        );
        assert_eq!(slices(&ft), ["01234", "89"]);
        assert!(matches!(ft.entities[1].kind, TextEntityKind::Underline));
    }

    #[test]
    fn pre_code_keeps_language() {
        let pre = TextEntityType::PreCode(types::TextEntityTypePreCode {
            language: "rust".into(),
        });
        let ft = convert("fn main() {}", vec![entity(0, 12, pre)]);
        assert_eq!(
            ft.entities[0].kind,
            TextEntityKind::Pre {
                language: "rust".into()
            }
        );
    }
}
