use chrono::{
    DateTime, Datelike, Days, FixedOffset, Months, NaiveDate, NaiveTime, Timelike, Weekday,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarViewKind {
    Year,
    #[default]
    Month,
    Week,
    Day,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

pub fn week_dates(anchor: NaiveDate, first_weekday: Weekday) -> Vec<NaiveDate> {
    let offset =
        (anchor.weekday().num_days_from_sunday() + 7 - first_weekday.num_days_from_sunday()) % 7;
    let Some(start) = anchor.checked_sub_days(Days::new(u64::from(offset))) else {
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
