use chrono::{
    DateTime, Datelike, Days, FixedOffset, Months, NaiveDate, NaiveTime, Timelike, Weekday,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CalendarViewKind {
    Year,
    #[default]
    Month,
    Week,
    Day,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TimeFormat {
    #[default]
    Locale,
    Hours12,
    Hours24,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarLocale {
    pub months: [&'static str; 12],
    pub weekdays: [&'static str; 7],
    pub first_weekday: Weekday,
    pub time_format: TimeFormat,
    /// Toolbar and accessibility strings.
    pub labels: CalendarLabels,
}

/// Translated toolbar and accessibility strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarLabels {
    /// Accessible names of the previous button, indexed Year, Month, Week, Day.
    pub previous: [&'static str; 4],
    /// Accessible names of the next button, indexed Year, Month, Week, Day.
    pub next: [&'static str; 4],
    /// View switcher labels, indexed Year, Month, Week, Day.
    pub views: [&'static str; 4],
    pub today: &'static str,
    pub all_day: &'static str,
    pub new_event: &'static str,
    pub edit_preview: &'static str,
    pub close: &'static str,
    pub events_for: &'static str,
    pub locked: &'static str,
    pub continues_before: &'static str,
    pub continues_after: &'static str,
    pub no_events: &'static str,
    pub one_event: &'static str,
    pub events: &'static str,
}

impl CalendarLabels {
    pub const EN: Self = Self {
        previous: [
            "Previous year",
            "Previous month",
            "Previous week",
            "Previous day",
        ],
        next: ["Next year", "Next month", "Next week", "Next day"],
        views: ["Year", "Month", "Week", "Day"],
        today: "Today",
        all_day: "All day",
        new_event: "New event",
        edit_preview: "Edit preview",
        close: "Close",
        events_for: "Events for",
        locked: "locked",
        continues_before: "continues from the previous day",
        continues_after: "continues into the next day",
        no_events: "no events",
        one_event: "1 event",
        events: "events",
    };
}

impl CalendarLocale {
    pub const EN_US: Self = Self {
        months: [
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
        ],
        weekdays: [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ],
        first_weekday: Weekday::Sun,
        time_format: TimeFormat::Hours12,
        labels: CalendarLabels::EN,
    };

    pub const EN_GB: Self = Self {
        first_weekday: Weekday::Mon,
        time_format: TimeFormat::Hours24,
        ..Self::EN_US
    };

    pub fn month(&self, month: u32) -> &str {
        self.months
            .get(month.saturating_sub(1) as usize)
            .copied()
            .unwrap_or("")
    }

    pub fn weekday(&self, weekday: Weekday) -> &str {
        self.weekdays
            .get(weekday.num_days_from_sunday() as usize)
            .copied()
            .unwrap_or("")
    }

    /// The first three characters of the weekday name, split on a character boundary.
    pub fn short_weekday(&self, weekday: Weekday) -> &'static str {
        let name: &'static str = self
            .weekdays
            .get(weekday.num_days_from_sunday() as usize)
            .copied()
            .unwrap_or("");
        let end = name
            .char_indices()
            .nth(3)
            .map_or(name.len(), |(index, _)| index);
        name.get(..end).unwrap_or(name)
    }

    /// Full accessible date, e.g. `Saturday, November 21, 2026`.
    pub fn date_label(&self, date: NaiveDate) -> String {
        format!(
            "{}, {} {}, {}",
            self.weekday(date.weekday()),
            self.month(date.month()),
            date.day(),
            date.year()
        )
    }

    /// Localized event count, e.g. `no events`, `1 event`, `3 events`.
    pub fn count_label(&self, count: usize) -> String {
        match count {
            0 => self.labels.no_events.to_string(),
            1 => self.labels.one_event.to_string(),
            count => format!("{count} {}", self.labels.events),
        }
    }

    pub const fn resolved_time_format(&self, override_format: TimeFormat) -> TimeFormat {
        match override_format {
            TimeFormat::Locale => self.time_format,
            value => value,
        }
    }
}

impl Default for CalendarLocale {
    fn default() -> Self {
        Self::EN_US
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateCell {
    pub date: NaiveDate,
    pub in_month: bool,
    pub iso_week: i32,
}

/// First date of the display week containing `anchor`.
pub fn week_start(anchor: NaiveDate, first_weekday: Weekday) -> Option<NaiveDate> {
    let offset =
        (anchor.weekday().num_days_from_sunday() + 7 - first_weekday.num_days_from_sunday()) % 7;
    anchor.checked_sub_days(Days::new(u64::from(offset)))
}

pub fn week_dates(anchor: NaiveDate, first_weekday: Weekday) -> Vec<NaiveDate> {
    let Some(start) = week_start(anchor, first_weekday) else {
        return Vec::new();
    };
    (0..7)
        .filter_map(|day| start.checked_add_days(Days::new(day)))
        .collect()
}

pub fn month_cells(anchor: NaiveDate, first_weekday: Weekday) -> Vec<DateCell> {
    let Some(first) = anchor.with_day(1) else {
        return Vec::new();
    };
    let Some(start) = week_dates(first, first_weekday).first().copied() else {
        return Vec::new();
    };
    (0..42)
        .filter_map(|day| start.checked_add_days(Days::new(day)))
        .map(|date| DateCell {
            date,
            in_month: date.month() == anchor.month() && date.year() == anchor.year(),
            iso_week: date.iso_week().week() as i32,
        })
        .collect()
}

pub fn navigate(anchor: NaiveDate, view: CalendarViewKind, direction: i32) -> NaiveDate {
    if direction == 0 {
        return anchor;
    }
    let forward = direction > 0;
    match view {
        CalendarViewKind::Year => {
            if forward {
                anchor.checked_add_months(Months::new(12))
            } else {
                anchor.checked_sub_months(Months::new(12))
            }
        }
        CalendarViewKind::Month => {
            if forward {
                anchor.checked_add_months(Months::new(1))
            } else {
                anchor.checked_sub_months(Months::new(1))
            }
        }
        CalendarViewKind::Week => {
            if forward {
                anchor.checked_add_days(Days::new(7))
            } else {
                anchor.checked_sub_days(Days::new(7))
            }
        }
        CalendarViewKind::Day => {
            if forward {
                anchor.checked_add_days(Days::new(1))
            } else {
                anchor.checked_sub_days(Days::new(1))
            }
        }
    }
    .unwrap_or(anchor)
}

pub const fn initial_scroll_minutes(viewport_minutes: u32) -> u32 {
    if viewport_minutes >= 1_440 { 0 } else { 480 }
}

pub fn format_hour(hour: u32, format: TimeFormat) -> String {
    match format {
        TimeFormat::Hours12 => {
            let suffix = if hour < 12 { "AM" } else { "PM" };
            let hour = match hour % 12 {
                0 => 12,
                value => value,
            };
            format!("{hour} {suffix}")
        }
        TimeFormat::Locale | TimeFormat::Hours24 => format!("{hour:02}:00"),
    }
}

pub fn format_local_time(time: DateTime<FixedOffset>, format: TimeFormat) -> String {
    match format {
        TimeFormat::Hours12 => time.format("%-I:%M %p").to_string(),
        TimeFormat::Locale | TimeFormat::Hours24 => time.format("%H:%M").to_string(),
    }
}

/// The hour-gutter label for `hour` (0..=23) without allocating.
pub fn hour_label(hour: u32, format: TimeFormat) -> &'static str {
    const H12: [&str; 24] = [
        "12 AM", "1 AM", "2 AM", "3 AM", "4 AM", "5 AM", "6 AM", "7 AM", "8 AM", "9 AM", "10 AM",
        "11 AM", "12 PM", "1 PM", "2 PM", "3 PM", "4 PM", "5 PM", "6 PM", "7 PM", "8 PM", "9 PM",
        "10 PM", "11 PM",
    ];
    const H24: [&str; 24] = [
        "00:00", "01:00", "02:00", "03:00", "04:00", "05:00", "06:00", "07:00", "08:00", "09:00",
        "10:00", "11:00", "12:00", "13:00", "14:00", "15:00", "16:00", "17:00", "18:00", "19:00",
        "20:00", "21:00", "22:00", "23:00",
    ];
    let table = match format {
        TimeFormat::Hours12 => &H12,
        TimeFormat::Locale | TimeFormat::Hours24 => &H24,
    };
    table.get(hour as usize).copied().unwrap_or("")
}

/// `start – end` in the resolved format, e.g. `9:00 AM – 10:30 AM`.
pub fn format_time_range(
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
    format: TimeFormat,
) -> String {
    format!(
        "{} – {}",
        format_local_time(start, format),
        format_local_time(end, format)
    )
}

/// ISO-8601 week number of `date` (1..=53), independent of the display week start.
pub fn iso_week(date: NaiveDate) -> u32 {
    date.iso_week().week()
}

/// ISO-8601 week shown for a display row of seven dates starting at `row_start`.
///
/// A row that does not start on Monday straddles two ISO weeks; it is labeled with
/// the week that owns most of its days, which is the week of its Thursday.
pub fn row_iso_week(row_start: NaiveDate) -> u32 {
    let offset =
        (Weekday::Thu.num_days_from_monday() + 7 - row_start.weekday().num_days_from_monday()) % 7;
    row_start
        .checked_add_days(Days::new(u64::from(offset)))
        .map_or_else(|| iso_week(row_start), iso_week)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkingHours {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl WorkingHours {
    pub fn new(start: NaiveTime, end: NaiveTime) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }

    pub fn contains(&self, time: NaiveTime) -> bool {
        self.start <= time && time < self.end
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BusinessHours {
    weekdays: [Option<WorkingHours>; 7],
}

impl std::fmt::Debug for BusinessHours {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusinessHours")
            .field("weekdays", &self.weekdays)
            .finish()
    }
}

impl Default for BusinessHours {
    fn default() -> Self {
        let standard = NaiveTime::from_hms_opt(9, 0, 0);
        let end = NaiveTime::from_hms_opt(17, 0, 0);
        let working = standard
            .zip(end)
            .and_then(|(start, end)| WorkingHours::new(start, end));
        Self {
            weekdays: [None, working, working, working, working, working, None],
        }
    }
}

impl BusinessHours {
    pub fn for_weekday(&self, weekday: Weekday) -> Option<WorkingHours> {
        self.weekdays
            .get(weekday.num_days_from_sunday() as usize)
            .copied()
            .flatten()
    }

    pub fn set_weekday(&mut self, weekday: Weekday, hours: Option<WorkingHours>) {
        if let Some(value) = self
            .weekdays
            .get_mut(weekday.num_days_from_sunday() as usize)
        {
            *value = hours;
        }
    }

    pub fn is_working(&self, weekday: Weekday, time: NaiveTime) -> bool {
        match self.for_weekday(weekday) {
            Some(hours) => hours.contains(time),
            None => false,
        }
    }

    /// Non-working minute ranges `[start, end)` of `weekday` in display wall-clock
    /// minutes. At most two ranges (before and after work); a day without working
    /// hours is one full-day range. Empty ranges are omitted.
    pub fn non_working_minutes(&self, weekday: Weekday) -> impl Iterator<Item = (i32, i32)> {
        let ranges = match self.for_weekday(weekday) {
            None => [(0, 1_440), (0, 0)],
            Some(hours) => {
                let start = (hours.start.hour() * 60 + hours.start.minute()) as i32;
                let end = (hours.end.hour() * 60 + hours.end.minute()) as i32;
                [(0, start), (end, 1_440)]
            }
        };
        ranges.into_iter().filter(|(start, end)| start < end)
    }
}

pub fn minute_of_day(time: DateTime<FixedOffset>) -> i32 {
    (time.hour() * 60 + time.minute()) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap_or(NaiveDate::MIN)
    }

    #[test]
    fn month_is_six_weeks_on_every_week_start() {
        for first in [Weekday::Sun, Weekday::Mon, Weekday::Sat] {
            let cells = month_cells(date(21), first);
            assert_eq!(cells.len(), 42);
            assert_eq!(cells.first().map(|cell| cell.date.weekday()), Some(first));
            assert!(cells.iter().all(|cell| cell.iso_week > 0));
            assert!(cells.iter().any(|cell| cell.in_month));
        }
    }

    #[test]
    fn navigation_handles_leap_months_and_week_boundaries() {
        let leap = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap_or(NaiveDate::MIN);
        assert_eq!(
            navigate(leap, CalendarViewKind::Day, 1),
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap_or(NaiveDate::MIN)
        );
        assert_eq!(
            navigate(leap, CalendarViewKind::Month, 1),
            NaiveDate::from_ymd_opt(2024, 3, 29).unwrap_or(NaiveDate::MIN)
        );
        assert_eq!(
            navigate(leap, CalendarViewKind::Year, 1),
            NaiveDate::from_ymd_opt(2025, 2, 28).unwrap_or(NaiveDate::MIN)
        );
        assert_eq!(
            navigate(date(21), CalendarViewKind::Week, -24),
            date(14),
            "navigation uses direction sign, not absolute step count"
        );
    }

    #[test]
    fn locale_and_time_format_are_respected() {
        assert_eq!(CalendarLocale::EN_US.first_weekday, Weekday::Sun);
        assert_eq!(CalendarLocale::EN_GB.first_weekday, Weekday::Mon);
        assert_eq!(format_hour(0, TimeFormat::Hours12), "12 AM");
        assert_eq!(format_hour(12, TimeFormat::Hours12), "12 PM");
        assert_eq!(format_hour(23, TimeFormat::Hours24), "23:00");
    }

    #[test]
    fn business_hours_cover_each_weekday_without_index_panics() {
        let mut hours = BusinessHours::default();
        let weekdays = [
            Weekday::Sun,
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
        ];
        for (index, weekday) in weekdays.into_iter().enumerate() {
            let expected = index != 0 && index != 6;
            assert_eq!(hours.for_weekday(weekday).is_some(), expected);
            hours.set_weekday(weekday, None);
        }
        for weekday in weekdays {
            assert!(hours.for_weekday(weekday).is_none());
        }
    }

    #[test]
    fn iso_weeks_use_iso_rules_not_the_display_week_start() {
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap_or(NaiveDate::MIN);
        // 2026 starts on a Thursday, so it has 53 ISO weeks and 2027-01-01 is in W53.
        assert_eq!(iso_week(day(2027, 1, 1)), 53);
        assert_eq!(iso_week(day(2026, 12, 28)), 53);
        assert_eq!(iso_week(day(2027, 1, 4)), 1);
        assert_eq!(iso_week(day(2024, 12, 30)), 1, "late December can be W1");
        // A Sunday-start row (Sun 2026-11-22 .. Sat 11-28) is mostly ISO week 48,
        // although its Sunday belongs to week 47.
        assert_eq!(iso_week(day(2026, 11, 22)), 47);
        assert_eq!(row_iso_week(day(2026, 11, 22)), 48);
        assert_eq!(row_iso_week(day(2026, 11, 23)), 48, "Monday rows are exact");
        assert_eq!(row_iso_week(day(2026, 11, 21)), 48, "Saturday rows");
        assert_eq!(
            row_iso_week(day(2026, 12, 27)),
            53,
            "Sunday row across New Year"
        );
    }

    #[test]
    fn hour_labels_and_ranges_follow_the_resolved_format() {
        for hour in 0..24 {
            assert_eq!(
                hour_label(hour, TimeFormat::Hours12),
                format_hour(hour, TimeFormat::Hours12)
            );
            assert_eq!(
                hour_label(hour, TimeFormat::Hours24),
                format_hour(hour, TimeFormat::Hours24)
            );
        }
        assert_eq!(hour_label(24, TimeFormat::Hours24), "");
        let offset = chrono::Offset::fix(&chrono::Utc);
        let at = |h, m| {
            date(21)
                .and_hms_opt(h, m, 0)
                .and_then(|t| t.and_local_timezone(offset).single())
        };
        let (Some(start), Some(end)) = (at(9, 0), at(13, 30)) else {
            panic!("fixture times")
        };
        assert_eq!(
            format_time_range(start, end, TimeFormat::Hours12),
            "9:00 AM – 1:30 PM"
        );
        assert_eq!(
            format_time_range(start, end, TimeFormat::Hours24),
            "09:00 – 13:30"
        );
        assert_eq!(
            CalendarLocale::EN_US.resolved_time_format(TimeFormat::Locale),
            TimeFormat::Hours12
        );
        assert_eq!(
            CalendarLocale::EN_GB.resolved_time_format(TimeFormat::Locale),
            TimeFormat::Hours24
        );
        assert_eq!(
            CalendarLocale::EN_GB.resolved_time_format(TimeFormat::Hours12),
            TimeFormat::Hours12
        );
    }

    #[test]
    fn labels_are_localized_and_split_on_char_boundaries() {
        let locale = CalendarLocale::EN_US;
        assert_eq!(locale.date_label(date(21)), "Saturday, November 21, 2026");
        assert_eq!(locale.short_weekday(Weekday::Wed), "Wed");
        let mut accented = CalendarLocale::EN_US;
        accented.weekdays[3] = "Mércoles";
        assert_eq!(accented.short_weekday(Weekday::Wed), "Mér");
        accented.weekdays[3] = "Mi";
        assert_eq!(accented.short_weekday(Weekday::Wed), "Mi");
        assert_eq!(locale.count_label(0), "no events");
        assert_eq!(locale.count_label(1), "1 event");
        assert_eq!(locale.count_label(4), "4 events");
    }

    #[test]
    fn non_working_ranges_shade_around_each_weekday() {
        let mut hours = BusinessHours::default();
        let ranges: Vec<_> = hours.non_working_minutes(Weekday::Mon).collect();
        assert_eq!(ranges, vec![(0, 540), (1_020, 1_440)]);
        let weekend: Vec<_> = hours.non_working_minutes(Weekday::Sun).collect();
        assert_eq!(weekend, vec![(0, 1_440)]);
        let half_past = NaiveTime::from_hms_opt(9, 30, 0)
            .zip(NaiveTime::from_hms_opt(23, 59, 0))
            .and_then(|(start, end)| WorkingHours::new(start, end));
        hours.set_weekday(Weekday::Tue, half_past);
        let ranges: Vec<_> = hours.non_working_minutes(Weekday::Tue).collect();
        assert_eq!(ranges, vec![(0, 570), (1_439, 1_440)]);
        let early = NaiveTime::from_hms_opt(0, 0, 0)
            .zip(NaiveTime::from_hms_opt(12, 0, 0))
            .and_then(|(start, end)| WorkingHours::new(start, end));
        hours.set_weekday(Weekday::Wed, early);
        let ranges: Vec<_> = hours.non_working_minutes(Weekday::Wed).collect();
        assert_eq!(ranges, vec![(720, 1_440)], "empty ranges are omitted");
    }

    proptest::proptest! {
        #[test]
        fn six_week_month_stays_ordered(start in 0u8..7) {
            let first = Weekday::try_from(start).unwrap_or(Weekday::Sun);
            let cells = month_cells(date(21), first);
            proptest::prop_assert_eq!(cells.len(), 42);
            proptest::prop_assert!(cells.windows(2).all(|pair| pair[0].date < pair[1].date));
        }
    }
}
