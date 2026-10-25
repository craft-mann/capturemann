//! Locale-aware formatting for the UI: sizes, decimal numbers and day labels.
//!
//! A per-language table decides the decimal separator, the month names and the words for today
//! and yesterday. No ICU. Unit symbols (`KB`, `MB`, `GB`) and timecodes are not localised. The
//! English output is byte-identical to the app's former `format_size` and `day_label_at`
//! (`apps/desktop/src/library.rs`); tests there compare the two.
//!
//! Every function takes the language explicitly (`*_in`) and has a shorthand that uses the
//! current one ([`crate::language`]).

use crate::{language, Lang};

/// The decimal separator of a language.
pub fn decimal_separator(lang: Lang) -> char {
    match lang {
        Lang::Vi => ',',
        Lang::En | Lang::EnXa => '.',
    }
}

/// A number with `places` decimals and the language's separator: `2.4` or `2,4`.
pub fn decimal_in(lang: Lang, value: f64, places: usize) -> String {
    let s = format!("{value:.places$}");
    match decimal_separator(lang) {
        '.' => s,
        sep => s.replace('.', &sep.to_string()),
    }
}

/// [`decimal_in`] in the current language.
pub fn decimal(value: f64, places: usize) -> String {
    decimal_in(language(), value, places)
}

/// A file size, MB = 10^6 bytes (specs convention): `512 B`, `12 KB`, `2.4 MB`, `1.23 GB`.
pub fn size_in(lang: Lang, bytes: u64) -> String {
    const KB: f64 = 1_000.;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{} MB", decimal_in(lang, b / (KB * KB), 1))
    } else {
        format!("{} GB", decimal_in(lang, b / (KB * KB * KB), 2))
    }
}

/// [`size_in`] in the current language.
pub fn size(bytes: u64) -> String {
    size_in(language(), bytes)
}

/// The short month name, `month` 1 to 12 (out of range clamps).
pub fn month_short(lang: Lang, month: u8) -> String {
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m = usize::from(month.clamp(1, 12));
    match lang {
        Lang::Vi => format!("thg {m}"),
        Lang::En | Lang::EnXa => EN[m - 1].to_owned(),
    }
}

/// A calendar date, already in the user's local time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ymd {
    pub year: i32,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
}

impl Ymd {
    pub fn new(year: i32, month: u8, day: u8) -> Self {
        Ymd { year, month, day }
    }

    /// The day before.
    pub fn previous(self) -> Ymd {
        if self.day > 1 {
            return Ymd {
                day: self.day - 1,
                ..self
            };
        }
        let (year, month) = if self.month > 1 {
            (self.year, self.month - 1)
        } else {
            (self.year - 1, 12)
        };
        Ymd {
            year,
            month,
            day: days_in_month(year, month),
        }
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A day header: `Today`, `Yesterday`, else `Oct 12` (this year) or `Oct 12, 2025`; Vietnamese
/// `Hôm nay`, `Hôm qua`, `12 thg 10`, `12 thg 10, 2025`.
pub fn day_label_in(lang: Lang, t: Ymd, now: Ymd) -> String {
    let vi = lang == Lang::Vi;
    if t == now {
        return if vi { "H\u{f4}m nay" } else { "Today" }.to_owned();
    }
    if now.previous() == t {
        return if vi { "H\u{f4}m qua" } else { "Yesterday" }.to_owned();
    }
    let month = month_short(lang, t.month);
    let date = if vi {
        format!("{} {month}", t.day)
    } else {
        format!("{month} {}", t.day)
    };
    if t.year == now.year {
        date
    } else {
        format!("{date}, {}", t.year)
    }
}

/// [`day_label_in`] in the current language.
pub fn day_label(t: Ymd, now: Ymd) -> String {
    day_label_in(language(), t, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inputs and expected strings of the `formatting` test in `library.rs`.
    #[test]
    fn english_sizes_match_the_library_formatter() {
        assert_eq!(size_in(Lang::En, 512), "512 B");
        assert_eq!(size_in(Lang::En, 2_400_000), "2.4 MB");
        assert_eq!(size_in(Lang::En, 1_230_000_000), "1.23 GB");
        assert_eq!(size_in(Lang::En, 999), "999 B");
        assert_eq!(size_in(Lang::En, 12_345), "12 KB");
    }

    #[test]
    fn vietnamese_sizes_use_a_decimal_comma_and_keep_the_unit() {
        assert_eq!(size_in(Lang::Vi, 512), "512 B");
        assert_eq!(size_in(Lang::Vi, 12_345), "12 KB");
        assert_eq!(size_in(Lang::Vi, 2_400_000), "2,4 MB");
        assert_eq!(size_in(Lang::Vi, 1_230_000_000), "1,23 GB");
    }

    #[test]
    fn decimals() {
        assert_eq!(decimal_in(Lang::En, 2.5, 1), "2.5");
        assert_eq!(decimal_in(Lang::Vi, 2.5, 1), "2,5");
        assert_eq!(decimal_in(Lang::Vi, 3.0, 0), "3");
        assert_eq!(decimal_in(Lang::En, -0.25, 2), "-0.25");
    }

    /// The inputs and expected strings of the `day_labels` test in `library.rs`
    /// (now = 2026-10-23, UTC).
    #[test]
    fn english_day_labels_match_the_library_formatter() {
        let now = Ymd::new(2026, 10, 23);
        let l = |y, m, d| day_label_in(Lang::En, Ymd::new(y, m, d), now);
        assert_eq!(l(2026, 10, 23), "Today");
        assert_eq!(l(2026, 10, 22), "Yesterday");
        assert_eq!(l(2026, 1, 5), "Jan 5");
        assert_eq!(l(2025, 12, 31), "Dec 31, 2025");
    }

    #[test]
    fn vietnamese_day_labels() {
        let now = Ymd::new(2026, 10, 23);
        let l = |y, m, d| day_label_in(Lang::Vi, Ymd::new(y, m, d), now);
        assert_eq!(l(2026, 10, 23), "Hôm nay");
        assert_eq!(l(2026, 10, 22), "Hôm qua");
        assert_eq!(l(2026, 10, 12), "12 thg 10");
        assert_eq!(l(2025, 12, 31), "31 thg 12, 2025");
    }

    #[test]
    fn yesterday_crosses_month_and_year_and_leap_days() {
        let y = |now: Ymd, t: Ymd| day_label_in(Lang::En, t, now) == "Yesterday";
        assert!(y(Ymd::new(2026, 11, 1), Ymd::new(2026, 10, 31)));
        assert!(y(Ymd::new(2026, 1, 1), Ymd::new(2025, 12, 31)));
        assert!(y(Ymd::new(2024, 3, 1), Ymd::new(2024, 2, 29)));
        assert!(y(Ymd::new(2026, 3, 1), Ymd::new(2026, 2, 28)));
        assert!(!y(Ymd::new(2026, 3, 1), Ymd::new(2026, 2, 27)));
    }

    #[test]
    fn current_language_shorthands_follow_the_override() {
        use crate::with_language;
        assert_eq!(with_language(Lang::Vi, || size(2_400_000)), "2,4 MB");
        assert_eq!(with_language(Lang::En, || size(2_400_000)), "2.4 MB");
        let d = Ymd::new(2026, 10, 23);
        assert_eq!(with_language(Lang::Vi, || day_label(d, d)), "Hôm nay");
    }
}
