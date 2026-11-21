//! Locale tables for the date and time pickers: month and weekday names, the first day
//! of the week, the clock, the default display format and every user-facing label.
//!
//! Pure data (no GPUI). Kit's calendar takes its names from a global `rust-i18n` locale;
//! the GUIraffe pickers take them from a [`DateLocale`] per picker instead, so two pickers
//! in one window can show different locales.

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike, Weekday};

/// 12- or 24-hour clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HourFormat {
    /// `1:30 PM`.
    H12,
    /// `13:30`.
    #[default]
    H24,
}

/// User-facing words the pickers show or announce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerLabels {
    pub previous_month: &'static str,
    pub next_month: &'static str,
    pub clear: &'static str,
    pub time: &'static str,
    pub start_time: &'static str,
    pub end_time: &'static str,
    pub done: &'static str,
    pub date_placeholder: &'static str,
    pub range_placeholder: &'static str,
    pub time_placeholder: &'static str,
    pub today: &'static str,
    pub yesterday: &'static str,
    pub tomorrow: &'static str,
    pub last_7_days: &'static str,
    pub last_30_days: &'static str,
    pub this_week: &'static str,
    pub last_week: &'static str,
    pub this_month: &'static str,
    pub last_month: &'static str,
    pub this_year: &'static str,
    pub custom: &'static str,
}

/// Names, conventions and labels of one locale.
///
/// `date_format` uses the pattern tokens of [`format_pattern`] (Mantine / dayjs style:
/// `YYYY MMMM MMM MM M DD D dddd ddd HH H hh h mm ss A`, `[literal]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateLocale {
    pub code: &'static str,
    pub months: [&'static str; 12],
    pub months_short: [&'static str; 12],
    /// Sunday first, like `Weekday::num_days_from_sunday`.
    pub weekdays: [&'static str; 7],
    /// Calendar column headers, Sunday first.
    pub weekdays_short: [&'static str; 7],
    pub first_day_of_week: Weekday,
    pub hour_format: HourFormat,
    pub date_format: &'static str,
    /// The long, unambiguous form read out for each calendar day.
    pub long_date_format: &'static str,
    pub am: &'static str,
    pub pm: &'static str,
    /// Placed between the two ends of a range.
    pub range_separator: &'static str,
    pub labels: PickerLabels,
}

impl Default for DateLocale {
    fn default() -> Self {
        Self::en_us()
    }
}

const EN_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const EN_MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const EN_WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const EN_LABELS: PickerLabels = PickerLabels {
    previous_month: "Previous month",
    next_month: "Next month",
    clear: "Clear",
    time: "Time",
    start_time: "Start time",
    end_time: "End time",
    done: "Done",
    date_placeholder: "Pick a date",
    range_placeholder: "Pick dates",
    time_placeholder: "--:--",
    today: "Today",
    yesterday: "Yesterday",
    tomorrow: "Tomorrow",
    last_7_days: "Last 7 days",
    last_30_days: "Last 30 days",
    this_week: "This week",
    last_week: "Last week",
    this_month: "This month",
    last_month: "Last month",
    this_year: "This year",
    custom: "Custom",
};

impl DateLocale {
    /// English (United States): weeks start on Sunday, 12-hour clock, `Mar 9, 2026`.
    pub fn en_us() -> Self {
        Self {
            code: "en-US",
            months: EN_MONTHS,
            months_short: EN_MONTHS_SHORT,
            weekdays: EN_WEEKDAYS,
            weekdays_short: ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"],
            first_day_of_week: Weekday::Sun,
            hour_format: HourFormat::H12,
            date_format: "MMM D, YYYY",
            long_date_format: "dddd, MMMM D, YYYY",
            am: "AM",
            pm: "PM",
            range_separator: " – ",
            labels: EN_LABELS,
        }
    }

    /// English (United Kingdom): weeks start on Monday, 24-hour clock, `9 Mar 2026`.
    pub fn en_gb() -> Self {
        Self {
            code: "en-GB",
            first_day_of_week: Weekday::Mon,
            hour_format: HourFormat::H24,
            date_format: "D MMM YYYY",
            long_date_format: "dddd D MMMM YYYY",
            ..Self::en_us()
        }
    }

    /// German: weeks start on Monday, 24-hour clock, `09.03.2026`.
    pub fn de() -> Self {
        Self {
            code: "de",
            months: [
                "Januar",
                "Februar",
                "März",
                "April",
                "Mai",
                "Juni",
                "Juli",
                "August",
                "September",
                "Oktober",
                "November",
                "Dezember",
            ],
            months_short: [
                "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
            ],
            weekdays: [
                "Sonntag",
                "Montag",
                "Dienstag",
                "Mittwoch",
                "Donnerstag",
                "Freitag",
                "Samstag",
            ],
            weekdays_short: ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
            first_day_of_week: Weekday::Mon,
            hour_format: HourFormat::H24,
            date_format: "DD.MM.YYYY",
            long_date_format: "dddd, D. MMMM YYYY",
            am: "AM",
            pm: "PM",
            range_separator: " – ",
            labels: PickerLabels {
                previous_month: "Vorheriger Monat",
                next_month: "Nächster Monat",
                clear: "Löschen",
                time: "Uhrzeit",
                start_time: "Startzeit",
                end_time: "Endzeit",
                done: "Fertig",
                date_placeholder: "Datum wählen",
                range_placeholder: "Zeitraum wählen",
                time_placeholder: "--:--",
                today: "Heute",
                yesterday: "Gestern",
                tomorrow: "Morgen",
                last_7_days: "Letzte 7 Tage",
                last_30_days: "Letzte 30 Tage",
                this_week: "Diese Woche",
                last_week: "Letzte Woche",
                this_month: "Dieser Monat",
                last_month: "Letzter Monat",
                this_year: "Dieses Jahr",
                custom: "Benutzerdefiniert",
            },
        }
    }

    /// French: weeks start on Monday, 24-hour clock, `9 mars 2026`.
    pub fn fr() -> Self {
        Self {
            code: "fr",
            months: [
                "janvier",
                "février",
                "mars",
                "avril",
                "mai",
                "juin",
                "juillet",
                "août",
                "septembre",
                "octobre",
                "novembre",
                "décembre",
            ],
            months_short: [
                "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.",
                "nov.", "déc.",
            ],
            weekdays: [
                "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
            ],
            weekdays_short: ["di", "lu", "ma", "me", "je", "ve", "sa"],
            first_day_of_week: Weekday::Mon,
            hour_format: HourFormat::H24,
            date_format: "D MMM YYYY",
            long_date_format: "dddd D MMMM YYYY",
            am: "AM",
            pm: "PM",
            range_separator: " – ",
            labels: PickerLabels {
                previous_month: "Mois précédent",
                next_month: "Mois suivant",
                clear: "Effacer",
                time: "Heure",
                start_time: "Heure de début",
                end_time: "Heure de fin",
                done: "Terminé",
                date_placeholder: "Choisir une date",
                range_placeholder: "Choisir des dates",
                time_placeholder: "--:--",
                today: "Aujourd’hui",
                yesterday: "Hier",
                tomorrow: "Demain",
                last_7_days: "7 derniers jours",
                last_30_days: "30 derniers jours",
                this_week: "Cette semaine",
                last_week: "Semaine dernière",
                this_month: "Ce mois-ci",
                last_month: "Mois dernier",
                this_year: "Cette année",
                custom: "Personnalisé",
            },
        }
    }

    /// The name of `month` (1–12); empty when out of range.
    pub fn month(&self, month: u32) -> &'static str {
        self.months
            .get((month as usize).wrapping_sub(1))
            .copied()
            .unwrap_or("")
    }

    /// The column header for `weekday` (0 = Sunday); wraps.
    pub fn weekday_short(&self, weekday: u32) -> &'static str {
        self.weekdays_short[(weekday % 7) as usize]
    }

    /// The time pattern for this locale's clock, optionally with seconds.
    pub fn time_format(&self, hour_format: HourFormat, seconds: bool) -> &'static str {
        match (hour_format, seconds) {
            (HourFormat::H24, false) => "HH:mm",
            (HourFormat::H24, true) => "HH:mm:ss",
            (HourFormat::H12, false) => "h:mm A",
            (HourFormat::H12, true) => "h:mm:ss A",
        }
    }
}

const TOKENS: [&str; 18] = [
    "YYYY", "MMMM", "dddd", "MMM", "ddd", "YY", "MM", "DD", "HH", "hh", "mm", "ss", "M", "D", "H",
    "h", "A", "a",
];

/// Format `date` and/or `time` with a dayjs-style `pattern` in `locale`.
///
/// Tokens: `YYYY` `YY` year, `MMMM` `MMM` `MM` `M` month, `DD` `D` day, `dddd` `ddd`
/// weekday, `HH` `H` 24-hour, `hh` `h` 12-hour, `mm` minute, `ss` second, `A` / `a`
/// AM/PM. Text in `[brackets]` is copied as is. A token whose part is missing (a time
/// token without a time) renders nothing.
pub fn format_pattern(
    pattern: &str,
    date: Option<NaiveDate>,
    time: Option<NaiveTime>,
    locale: &DateLocale,
) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut rest = pattern;
    while let Some(c) = rest.chars().next() {
        if c == '[' {
            let body = &rest[1..];
            let end = body.find(']').unwrap_or(body.len());
            out.push_str(&body[..end]);
            rest = body.get(end + 1..).unwrap_or("");
            continue;
        }
        if let Some(token) = TOKENS.iter().find(|t| rest.starts_with(**t)) {
            push_token(&mut out, token, date, time, locale);
            rest = &rest[token.len()..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn push_token(
    out: &mut String,
    token: &str,
    date: Option<NaiveDate>,
    time: Option<NaiveTime>,
    locale: &DateLocale,
) {
    use std::fmt::Write as _;
    let hour12 = |t: NaiveTime| match t.hour() % 12 {
        0 => 12,
        h => h,
    };
    // Writing into a String cannot fail.
    let _ = match (token, date, time) {
        ("YYYY", Some(d), _) => write!(out, "{:04}", d.year()),
        ("YY", Some(d), _) => write!(out, "{:02}", d.year().rem_euclid(100)),
        ("MMMM", Some(d), _) => write!(out, "{}", locale.month(d.month())),
        ("MMM", Some(d), _) => write!(out, "{}", locale.months_short[d.month0() as usize % 12]),
        ("MM", Some(d), _) => write!(out, "{:02}", d.month()),
        ("M", Some(d), _) => write!(out, "{}", d.month()),
        ("DD", Some(d), _) => write!(out, "{:02}", d.day()),
        ("D", Some(d), _) => write!(out, "{}", d.day()),
        ("dddd", Some(d), _) => write!(
            out,
            "{}",
            locale.weekdays[d.weekday().num_days_from_sunday() as usize]
        ),
        ("ddd", Some(d), _) => write!(
            out,
            "{}",
            locale.weekdays_short[d.weekday().num_days_from_sunday() as usize]
        ),
        ("HH", _, Some(t)) => write!(out, "{:02}", t.hour()),
        ("H", _, Some(t)) => write!(out, "{}", t.hour()),
        ("hh", _, Some(t)) => write!(out, "{:02}", hour12(t)),
        ("h", _, Some(t)) => write!(out, "{}", hour12(t)),
        ("mm", _, Some(t)) => write!(out, "{:02}", t.minute()),
        ("ss", _, Some(t)) => write!(out, "{:02}", t.second()),
        ("A" | "a", _, Some(t)) => {
            let p = if t.hour() < 12 { locale.am } else { locale.pm };
            if token == "a" {
                write!(out, "{}", p.to_lowercase())
            } else {
                write!(out, "{p}")
            }
        }
        _ => Ok(()),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
    }
    fn t(h: u32, m: u32, s: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, s).unwrap_or_default()
    }

    #[test]
    fn formats_every_token() {
        let en = DateLocale::en_us();
        let date = Some(d(2026, 3, 9));
        assert_eq!(
            format_pattern("MMM D, YYYY", date, None, &en),
            "Mar 9, 2026"
        );
        assert_eq!(
            format_pattern("dddd, MMMM DD YY", date, None, &en),
            "Monday, March 09 26"
        );
        assert_eq!(format_pattern("M/D ddd", date, None, &en), "3/9 Mo");
        let time = Some(t(0, 5, 7));
        assert_eq!(format_pattern("h:mm:ss A", None, time, &en), "12:05:07 AM");
        assert_eq!(
            format_pattern("hh:mm a", None, Some(t(13, 0, 0)), &en),
            "01:00 pm"
        );
        assert_eq!(
            format_pattern("HH:mm H", None, Some(t(9, 30, 0)), &en),
            "09:30 9"
        );
    }

    #[test]
    fn brackets_and_missing_parts() {
        let en = DateLocale::en_us();
        assert_eq!(
            format_pattern("[Day] D [of] MMMM", Some(d(2026, 1, 2)), None, &en),
            "Day 2 of January"
        );
        // An unclosed bracket copies the rest.
        assert_eq!(
            format_pattern("D [at", Some(d(2026, 1, 2)), None, &en),
            "2 at"
        );
        // Time tokens without a time render nothing.
        assert_eq!(
            format_pattern("D HH:mm", Some(d(2026, 1, 2)), None, &en),
            "2 :"
        );
    }

    #[test]
    fn locales_differ_in_names_and_conventions() {
        let date = Some(d(2026, 3, 9));
        let de = DateLocale::de();
        assert_eq!(
            format_pattern(de.date_format, date, None, &de),
            "09.03.2026"
        );
        assert_eq!(
            format_pattern(de.long_date_format, date, None, &de),
            "Montag, 9. März 2026"
        );
        let fr = DateLocale::fr();
        assert_eq!(
            format_pattern(fr.date_format, date, None, &fr),
            "9 mars 2026"
        );
        assert_eq!(DateLocale::en_us().first_day_of_week, Weekday::Sun);
        assert_eq!(DateLocale::en_gb().first_day_of_week, Weekday::Mon);
        assert_eq!(DateLocale::en_gb().hour_format, HourFormat::H24);
        assert_eq!(DateLocale::en_us().month(13), "");
        assert_eq!(DateLocale::en_us().weekday_short(8), "Mo");
    }
}
