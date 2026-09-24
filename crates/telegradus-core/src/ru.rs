//! Russian wording helpers: plural forms and durations.

/// Russian plural form for `n`: one (1, 21), few (2-4, 22-24), many (5-20, 25-30, ...).
pub(crate) fn plural<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    match (n % 10, n % 100) {
        (1, rem) if rem != 11 => one,
        (2..=4, rem) if !(12..=14).contains(&rem) => few,
        _ => many,
    }
}

/// `n` followed by the matching plural form, e.g. "5 минут".
pub(crate) fn count(n: u64, one: &str, few: &str, many: &str) -> String {
    format!("{n} {}", plural(n, one, few, many))
}

/// A wait time for "повторите через ...": seconds below a minute, otherwise
/// minutes rounded up, e.g. "30 секунд", "2 минуты", "1 час 5 минут".
pub(crate) fn wait_time(seconds: u64) -> String {
    if seconds < 60 {
        return count(seconds.max(1), "секунду", "секунды", "секунд");
    }
    let minutes = seconds.div_ceil(60);
    if minutes < 60 {
        return count(minutes, "минуту", "минуты", "минут");
    }
    let hours = count(minutes / 60, "час", "часа", "часов");
    match minutes % 60 {
        0 => hours,
        rest => format!("{hours} {}", count(rest, "минуту", "минуты", "минут")),
    }
}

/// A period in the largest whole unit, e.g. "1 день", "2 недели", "5 минут".
pub(crate) fn period(seconds: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    const MONTH: u64 = 30 * DAY;
    match seconds {
        s if s >= MONTH && s % MONTH == 0 => count(s / MONTH, "месяц", "месяца", "месяцев"),
        s if s >= WEEK && s % WEEK == 0 => count(s / WEEK, "неделя", "недели", "недель"),
        s if s >= DAY => count(s / DAY, "день", "дня", "дней"),
        s if s >= HOUR => count(s / HOUR, "час", "часа", "часов"),
        s if s >= MINUTE => count(s / MINUTE, "минута", "минуты", "минут"),
        s => count(s, "секунда", "секунды", "секунд"),
    }
}

/// A media or call length such as "0:07" or "1:02:03".
pub(crate) fn clock(seconds: i32) -> String {
    let seconds = seconds.max(0);
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_forms() {
        let forms = |n| plural(n, "минуту", "минуты", "минут");
        assert_eq!(forms(1), "минуту");
        assert_eq!(forms(2), "минуты");
        assert_eq!(forms(5), "минут");
        assert_eq!(forms(11), "минут");
        assert_eq!(forms(12), "минут");
        assert_eq!(forms(21), "минуту");
        assert_eq!(forms(24), "минуты");
        assert_eq!(forms(111), "минут");
    }

    #[test]
    fn wait_times() {
        assert_eq!(wait_time(0), "1 секунду");
        assert_eq!(wait_time(30), "30 секунд");
        assert_eq!(wait_time(61), "2 минуты");
        assert_eq!(wait_time(3600), "1 час");
        assert_eq!(wait_time(3900), "1 час 5 минут");
    }

    #[test]
    fn periods_and_clock() {
        assert_eq!(period(86_400), "1 день");
        assert_eq!(period(7 * 86_400), "1 неделя");
        assert_eq!(period(30 * 86_400), "1 месяц");
        assert_eq!(period(2 * 3600), "2 часа");
        assert_eq!(period(45), "45 секунд");
        assert_eq!(clock(7), "0:07");
        assert_eq!(clock(3723), "1:02:03");
    }
}
