//! Pure formatting helpers: dates, sizes, durations, initials, previews.
//!
//! Everything user-facing is Russian. Functions that depend on "now" take the
//! current local date explicitly so they stay deterministic and testable.

use std::borrow::Cow;

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone};
use telegradus_core::{MessageContent, MessagePreview};

const MONTHS_GENITIVE: [&str; 12] = [
    "января",
    "февраля",
    "марта",
    "апреля",
    "мая",
    "июня",
    "июля",
    "августа",
    "сентября",
    "октября",
    "ноября",
    "декабря",
];

const WEEKDAYS_SHORT: [&str; 7] = ["пн", "вт", "ср", "чт", "пт", "сб", "вс"];

/// Converts a Unix timestamp (seconds) to local wall-clock time.
pub fn local_datetime(timestamp: i64) -> NaiveDateTime {
    match Local.timestamp_opt(timestamp, 0).single() {
        Some(time) => time.naive_local(),
        None => DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_default()
            .naive_utc(),
    }
}

/// Today's local date.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// `"14:05"`.
pub fn time_hm(time: NaiveDateTime) -> String {
    time.format("%H:%M").to_string()
}

/// Label of a day separator in the message list.
pub fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        return "Сегодня".to_owned();
    }
    if today.pred_opt() == Some(day) {
        return "Вчера".to_owned();
    }
    let month = MONTHS_GENITIVE[day.month0() as usize];
    if day.year() == today.year() {
        format!("{} {month}", day.day())
    } else {
        format!("{} {month} {}", day.day(), day.year())
    }
}

/// Compact timestamp for the chat list: time today, weekday this week,
/// `dd.mm` this year and `dd.mm.yy` before.
pub fn list_time(time: NaiveDateTime, today: NaiveDate) -> String {
    let day = time.date();
    if day == today {
        return time_hm(time);
    }
    let days_ago = (today - day).num_days();
    if (1..7).contains(&days_ago) {
        return WEEKDAYS_SHORT[day.weekday().num_days_from_monday() as usize].to_owned();
    }
    if day.year() == today.year() {
        day.format("%d.%m").to_string()
    } else {
        day.format("%d.%m.%y").to_string()
    }
}

/// Human-readable file size with a Russian decimal comma: `"1,2 МБ"`.
pub fn file_size(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["КБ", "МБ", "ГБ", "ТБ"];
    if bytes < 1024 {
        return format!("{} Б", bytes.max(0));
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let formatted = if value < 10.0 {
        let rounded = (value * 10.0).round() / 10.0;
        if rounded >= 10.0 {
            "10".to_owned()
        } else {
            format!("{rounded:.1}").replace('.', ",")
        }
    } else {
        format!("{}", value.round() as i64)
    };
    format!("{formatted} {}", UNITS[unit])
}

/// `"0:42"`, `"12:05"`, `"1:02:03"`.
pub fn duration(seconds: i32) -> String {
    let seconds = seconds.max(0);
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Up to two uppercase initials from a chat title, skipping emoji and punctuation.
pub fn initials(title: &str) -> String {
    let letters: String = title
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() {
        "?".to_owned()
    } else {
        letters
    }
}

/// Deterministic avatar shade index for a chat id.
pub fn shade_index(id: i64) -> usize {
    // A cheap integer hash (splitmix64 finalizer) so neighbouring ids differ.
    let mut x = id as u64;
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x % 6) as usize
}

/// Collapses whitespace (including newlines) into single spaces.
pub fn one_line(text: &str) -> Cow<'_, str> {
    let needs_work = text.contains(['\n', '\r', '\t']) || text.contains("  ");
    if !needs_work {
        return Cow::Borrowed(text.trim());
    }
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    Cow::Owned(out)
}

/// Approximate advance of a character in ems for Inter Tight.
fn char_em(c: char) -> f32 {
    match c {
        ' ' | 'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | 'I' | 'ı' => 0.26,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' | '"' | 'г' => 0.36,
        'm' | 'w' | 'M' | 'W' | 'ш' | 'щ' | 'ж' | 'ю' | 'ы' | 'м' | 'Ш' | 'Щ' | 'Ж' | 'Ю' | 'Ы'
        | 'М' | 'Ф' | 'ф' | '@' | '%' => 0.80,
        c if c.is_ascii_digit() => 0.55,
        c if c.is_uppercase() => 0.64,
        c if (c as u32) >= 0x1F000 || ('\u{2600}'..='\u{27BF}').contains(&c) => 1.25,
        c if (c as u32) >= 0x2E80 => 1.0,
        _ => 0.52,
    }
}

/// Shortens `text` with an ellipsis so it fits roughly into `max_width` pixels
/// at `font_size`. Text widgets cannot ellipsize, so this estimate (plus
/// clipping) keeps one-line labels tidy.
pub fn truncate_to_width(text: &str, max_width: f32, font_size: f32) -> Cow<'_, str> {
    let budget = max_width / font_size;
    let ellipsis = 0.9;
    let mut used = 0.0;
    let mut cut = None;
    for (index, c) in text.char_indices() {
        used += char_em(c);
        if cut.is_none() && used > budget - ellipsis {
            cut = Some(index);
        }
        if used > budget {
            let end = cut.unwrap_or(index);
            let mut short = text[..end].trim_end().to_owned();
            short.push('…');
            return Cow::Owned(short);
        }
    }
    Cow::Borrowed(text)
}

/// One-line description of a message's content, for replies and previews.
pub fn content_preview(content: &MessageContent) -> Cow<'_, str> {
    let with_caption = |label: &'static str, caption: &str| -> Cow<'static, str> {
        let caption = one_line(caption);
        if caption.is_empty() {
            Cow::Borrowed(label)
        } else {
            Cow::Owned(format!("{label} · {caption}"))
        }
    };
    match content {
        MessageContent::Text(text) => one_line(&text.text),
        MessageContent::Photo { caption, .. } => with_caption("Фото", &caption.text),
        MessageContent::Document {
            file_name, caption, ..
        } => {
            if caption.text.is_empty() {
                Cow::Borrowed(file_name.as_str())
            } else {
                Cow::Owned(format!("{file_name} · {}", one_line(&caption.text)))
            }
        }
        MessageContent::Video { caption, .. } => with_caption("Видео", &caption.text),
        MessageContent::Animation { caption } => with_caption("GIF", &caption.text),
        MessageContent::Audio {
            title, performer, ..
        } => match (performer.is_empty(), title.is_empty()) {
            (false, false) => Cow::Owned(format!("{performer} — {title}")),
            (true, false) => Cow::Borrowed(title.as_str()),
            (false, true) => Cow::Borrowed(performer.as_str()),
            (true, true) => Cow::Borrowed("Аудио"),
        },
        MessageContent::Voice { caption, .. } => with_caption("Голосовое сообщение", &caption.text),
        MessageContent::VideoNote { .. } => Cow::Borrowed("Видеосообщение"),
        MessageContent::Sticker { emoji } => Cow::Owned(format!("{emoji} Стикер")),
        MessageContent::Service(text) => Cow::Borrowed(text.as_str()),
        MessageContent::Unsupported(label) => Cow::Borrowed(label.as_str()),
    }
}

/// Preview line of a chat in the list: optional prefix (sender or "Вы") and text.
pub fn preview_parts(preview: &MessagePreview) -> (Option<&str>, Cow<'_, str>) {
    let prefix = if preview.is_outgoing {
        Some("Вы")
    } else {
        preview.sender_name.as_deref()
    };
    (prefix, one_line(&preview.text))
}

/// Display size of a `width`x`height` picture that fits into a `max` square,
/// keeping its aspect ratio, never upscaling and never collapsing to a sliver.
pub fn media_size(width: i32, height: i32, max: f32) -> (f32, f32) {
    if width <= 0 || height <= 0 {
        return (max, (max * 0.66).round());
    }
    let (w, h) = (width as f32, height as f32);
    let scale = (max / w).min(max / h).min(1.0);
    ((w * scale).round().max(96.0), (h * scale).round().max(64.0))
}

/// Makes a clickable URL out of link text (`example.com` -> `https://example.com`).
pub fn normalize_url(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    let has_scheme = ["http://", "https://", "tg://", "mailto:", "tel:", "ftp://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme));
    if has_scheme {
        url.to_owned()
    } else {
        format!("https://{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        date(y, m, d).and_hms_opt(h, min, 0).unwrap()
    }

    #[test]
    fn day_labels() {
        let today = date(2026, 3, 14);
        assert_eq!(day_label(today, today), "Сегодня");
        assert_eq!(day_label(date(2026, 3, 13), today), "Вчера");
        assert_eq!(day_label(date(2026, 3, 12), today), "12 марта");
        assert_eq!(day_label(date(2026, 1, 1), today), "1 января");
        assert_eq!(day_label(date(2025, 12, 31), today), "31 декабря 2025");
        // Yesterday across a year boundary.
        assert_eq!(day_label(date(2025, 12, 31), date(2026, 1, 1)), "Вчера");
    }

    #[test]
    fn list_times() {
        let today = date(2026, 3, 14); // Saturday
        assert_eq!(list_time(at(2026, 3, 14, 9, 5), today), "09:05");
        assert_eq!(list_time(at(2026, 3, 13, 23, 0), today), "пт");
        assert_eq!(list_time(at(2026, 3, 9, 10, 0), today), "пн");
        assert_eq!(list_time(at(2026, 3, 8, 10, 0), today), "вс");
        assert_eq!(list_time(at(2026, 3, 7, 10, 0), today), "07.03");
        assert_eq!(list_time(at(2024, 11, 2, 10, 0), today), "02.11.24");
    }

    #[test]
    fn times() {
        assert_eq!(time_hm(at(2026, 1, 1, 7, 3)), "07:03");
        assert_eq!(time_hm(at(2026, 1, 1, 23, 59)), "23:59");
    }

    #[test]
    fn file_sizes() {
        assert_eq!(file_size(0), "0 Б");
        assert_eq!(file_size(-5), "0 Б");
        assert_eq!(file_size(1023), "1023 Б");
        assert_eq!(file_size(1024), "1,0 КБ");
        assert_eq!(file_size(1_250), "1,2 КБ");
        assert_eq!(file_size(10_240), "10 КБ");
        assert_eq!(file_size(1_048_576), "1,0 МБ");
        assert_eq!(file_size(3_565_158), "3,4 МБ");
        assert_eq!(file_size(734_003_200), "700 МБ");
        assert_eq!(file_size(1_181_116_006), "1,1 ГБ");
        // Rounds up to the next integer without printing "10,0".
        assert_eq!(file_size(10_220), "10 КБ");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(0), "0:00");
        assert_eq!(duration(42), "0:42");
        assert_eq!(duration(725), "12:05");
        assert_eq!(duration(3723), "1:02:03");
        assert_eq!(duration(-3), "0:00");
    }

    #[test]
    fn initials_from_titles() {
        assert_eq!(initials("Анна Смирнова"), "АС");
        assert_eq!(initials("мама"), "М");
        assert_eq!(initials("🚀 Rust Beginners Chat"), "RB");
        assert_eq!(initials("«Книжный клуб»"), "КК");
        assert_eq!(initials("🔥🔥"), "?");
        assert_eq!(initials(""), "?");
    }

    #[test]
    fn shade_is_deterministic_and_spread() {
        assert_eq!(shade_index(42), shade_index(42));
        let distinct: std::collections::HashSet<_> = (0..64).map(shade_index).collect();
        assert!(distinct.len() >= 5);
        assert!(distinct.iter().all(|&s| s < 6));
    }

    #[test]
    fn one_line_collapses_whitespace() {
        assert_eq!(one_line("  привет "), "привет");
        assert_eq!(one_line("a\nb\r\n  c\t d"), "a b c d");
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate_to_width("Коротко", 200.0, 13.0), "Коротко");
        let long = "Очень длинное сообщение, которое точно не поместится в строку списка";
        let short = truncate_to_width(long, 120.0, 13.0);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() < long.chars().count());
        assert!(long.starts_with(short.trim_end_matches('…')));
    }

    #[test]
    fn media_sizes() {
        assert_eq!(media_size(800, 533, 360.0), (360.0, 240.0));
        assert_eq!(media_size(540, 720, 360.0), (270.0, 360.0));
        // Small pictures are not upscaled.
        assert_eq!(media_size(200, 150, 360.0), (200.0, 150.0));
        // Extreme panoramas keep a usable height.
        assert_eq!(media_size(4000, 100, 360.0), (360.0, 64.0));
        assert_eq!(media_size(0, 0, 360.0), (360.0, 238.0));
    }

    #[test]
    fn urls() {
        assert_eq!(normalize_url("example.com/a"), "https://example.com/a");
        assert_eq!(normalize_url("HTTP://x.org"), "HTTP://x.org");
        assert_eq!(normalize_url("mailto:a@b.c"), "mailto:a@b.c");
    }

    #[test]
    fn previews() {
        use telegradus_core::FormattedText;
        let caption = FormattedText {
            text: "закат\nна море".into(),
            entities: vec![],
        };
        let file = telegradus_core::FileRef {
            id: 1,
            size: 0,
            local_path: None,
            is_downloading: false,
            downloaded_size: 0,
        };
        let photo = MessageContent::Photo {
            file,
            width: 1,
            height: 1,
            caption,
        };
        assert_eq!(content_preview(&photo), "Фото · закат на море");
        let sticker = MessageContent::Sticker {
            emoji: "🙂".into()
        };
        assert_eq!(content_preview(&sticker), "🙂 Стикер");
    }
}
