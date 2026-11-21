//! Pure date logic for the pickers (no GPUI): min/max/excluded-date rules, clamping,
//! range ordering, preset ranges, the calendar grid and keyboard cursor movement.

use std::{fmt, sync::Arc};

use chrono::{Datelike, Days, Months, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

use super::locale::DateLocale;

/// A caller predicate marking dates that cannot be picked (`true` = excluded).
///
/// `Send + Sync` because Kit's calendar matcher requires it.
pub type DatePredicate = Arc<dyn Fn(NaiveDate) -> bool + Send + Sync>;

/// How far [`DateRules::nearest_allowed`] scans for an allowed day (a little over a year), so
/// a predicate that excludes everything cannot loop forever.
pub const SCAN_LIMIT: u64 = 400;

/// Which dates may be picked: an inclusive `min`/`max` and an exclusion predicate.
#[derive(Clone, Default)]
pub struct DateRules {
    pub min: Option<NaiveDate>,
    pub max: Option<NaiveDate>,
    pub exclude: Option<DatePredicate>,
}

impl fmt::Debug for DateRules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DateRules")
            .field("min", &self.min)
            .field("max", &self.max)
            .field("exclude", &self.exclude.as_ref().map(|_| "fn"))
            .finish()
    }
}

impl PartialEq for DateRules {
    fn eq(&self, other: &Self) -> bool {
        let same_fn = match (&self.exclude, &other.exclude) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        self.min == other.min && self.max == other.max && same_fn
    }
}

impl DateRules {
    /// Rules with `min` and `max`, swapped when given in the wrong order (bad props are
    /// clamped, not a panic; spec/00 §7).
    pub fn new(min: Option<NaiveDate>, max: Option<NaiveDate>) -> Self {
        let (min, max) = match (min, max) {
            (Some(a), Some(b)) if a > b => {
                debug_assert!(false, "min date {a} is after max date {b}");
                (Some(b), Some(a))
            }
            other => other,
        };
        Self {
            min,
            max,
            exclude: None,
        }
    }

    pub fn with_exclude(mut self, exclude: Option<DatePredicate>) -> Self {
        self.exclude = exclude;
        self
    }

    /// Within `min..=max`.
    pub fn in_bounds(&self, d: NaiveDate) -> bool {
        self.min.is_none_or(|m| d >= m) && self.max.is_none_or(|m| d <= m)
    }

    /// Within bounds and not excluded.
    pub fn allows(&self, d: NaiveDate) -> bool {
        self.in_bounds(d) && !self.exclude.as_ref().is_some_and(|f| f(d))
    }

    /// `d` moved into `min..=max`.
    pub fn clamp(&self, d: NaiveDate) -> NaiveDate {
        let d = self.min.map_or(d, |m| d.max(m));
        self.max.map_or(d, |m| d.min(m))
    }

    /// The nearest allowed day from `from` (inclusive), scanning forward or backward within
    /// bounds.
    pub fn nearest_allowed(&self, from: NaiveDate, forward: bool) -> Option<NaiveDate> {
        let mut d = from;
        for _ in 0..SCAN_LIMIT {
            if !self.in_bounds(d) {
                return None;
            }
            if self.allows(d) {
                return Some(d);
            }
            d = if forward {
                d.succ_opt()?
            } else {
                d.pred_opt()?
            };
        }
        None
    }

    /// `d` clamped into bounds, then moved to the nearest allowed day (forward first).
    pub fn snap(&self, d: NaiveDate) -> Option<NaiveDate> {
        let d = self.clamp(d);
        self.nearest_allowed(d, true)
            .or_else(|| self.nearest_allowed(d, false))
    }

    /// A range ordered, clamped into bounds, with both ends moved inward onto allowed days.
    /// `None` when nothing of it is pickable. Excluded days *inside* the range are kept.
    pub fn clamp_range(&self, a: NaiveDate, b: NaiveDate) -> Option<DateRange> {
        let r = DateRange::new(a, b);
        if self.min.is_some_and(|m| r.end < m) || self.max.is_some_and(|m| r.start > m) {
            return None;
        }
        let start = self.nearest_allowed(self.clamp(r.start), true)?;
        let end = self.nearest_allowed(self.clamp(r.end), false)?;
        (start <= end).then_some(DateRange { start, end })
    }

    /// Whether any day of `year`/`month` is in bounds (for month navigation limits).
    pub fn month_in_bounds(&self, year: i32, month: u32) -> bool {
        let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
            return false;
        };
        let last = last_of_month(first);
        self.min.is_none_or(|m| last >= m) && self.max.is_none_or(|m| first <= m)
    }
}

fn last_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1)
        .and_then(|f| f.checked_add_months(Months::new(1)))
        .and_then(|n| n.pred_opt())
        .unwrap_or(d)
}

/// An inclusive, ordered date range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateRange {
    /// The range between `a` and `b`, in either order.
    pub fn new(a: NaiveDate, b: NaiveDate) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }
    pub fn contains(&self, d: NaiveDate) -> bool {
        self.start <= d && d <= self.end
    }
    /// Number of days, both ends included.
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }
}

/// An inclusive, ordered date-time range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateTimeRange {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

impl DateTimeRange {
    /// The range between `a` and `b`, in either order.
    pub fn new(a: NaiveDateTime, b: NaiveDateTime) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }
}

/// The next state of a range selection after the user picks `picked`.
///
/// This is Kit's calendar rule, which the pointer path runs: no start, or a complete
/// range, starts over; a pick on or after the start completes the range; a pick before
/// the start restarts from it. The result is always ordered.
pub fn pick_range(
    current: (Option<NaiveDate>, Option<NaiveDate>),
    picked: NaiveDate,
) -> (Option<NaiveDate>, Option<NaiveDate>) {
    match current {
        (Some(start), None) if picked >= start => (Some(start), Some(picked)),
        _ => (Some(picked), None),
    }
}

/// The range a hover preview shows while only the start is picked.
pub fn preview_range(start: NaiveDate, hovered: NaiveDate) -> Option<DateRange> {
    (hovered >= start).then_some(DateRange {
        start,
        end: hovered,
    })
}

/// A configurable quick choice in a picker's preset list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetRange {
    Today,
    Yesterday,
    Tomorrow,
    /// The last `n` days, today included.
    LastDays(u32),
    /// The next `n` days, today included.
    NextDays(u32),
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    Fixed(NaiveDate, NaiveDate),
    /// Clears the selection and leaves the calendar open for a manual pick.
    Custom,
}

impl PresetRange {
    /// The dates this preset stands for, relative to `today`; `None` for [`Self::Custom`].
    pub fn resolve(self, today: NaiveDate, first_day: Weekday) -> Option<DateRange> {
        let days = |n: u32| Days::new(u64::from(n.max(1) - 1));
        let week = week_start(today, first_day);
        let month = today.with_day(1)?;
        let r = |a: NaiveDate, b: NaiveDate| Some(DateRange::new(a, b));
        match self {
            Self::Today => r(today, today),
            Self::Yesterday => {
                let y = today.pred_opt()?;
                r(y, y)
            }
            Self::Tomorrow => {
                let t = today.succ_opt()?;
                r(t, t)
            }
            Self::LastDays(n) => r(today.checked_sub_days(days(n))?, today),
            Self::NextDays(n) => r(today, today.checked_add_days(days(n))?),
            Self::ThisWeek => r(week, week.checked_add_days(Days::new(6))?),
            Self::LastWeek => {
                let start = week.checked_sub_days(Days::new(7))?;
                r(start, week.pred_opt()?)
            }
            Self::ThisMonth => r(month, last_of_month(month)),
            Self::LastMonth => {
                let start = month.checked_sub_months(Months::new(1))?;
                r(start, month.pred_opt()?)
            }
            Self::ThisYear => r(
                NaiveDate::from_ymd_opt(today.year(), 1, 1)?,
                NaiveDate::from_ymd_opt(today.year(), 12, 31)?,
            ),
            Self::Fixed(a, b) => r(a, b),
            Self::Custom => None,
        }
    }
}

/// A labelled preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatePreset {
    pub label: String,
    pub range: PresetRange,
}

impl DatePreset {
    pub fn new(label: impl Into<String>, range: PresetRange) -> Self {
        Self {
            label: label.into(),
            range,
        }
    }

    /// Today, Last 7 days, Last 30 days, This month, Last month, Custom — in `locale`.
    pub fn standard_ranges(locale: &DateLocale) -> Vec<Self> {
        let l = &locale.labels;
        vec![
            Self::new(l.today, PresetRange::Today),
            Self::new(l.last_7_days, PresetRange::LastDays(7)),
            Self::new(l.last_30_days, PresetRange::LastDays(30)),
            Self::new(l.this_month, PresetRange::ThisMonth),
            Self::new(l.last_month, PresetRange::LastMonth),
            Self::new(l.custom, PresetRange::Custom),
        ]
    }

    /// Today, Yesterday, Tomorrow — for single-date pickers, in `locale`.
    pub fn standard_days(locale: &DateLocale) -> Vec<Self> {
        let l = &locale.labels;
        vec![
            Self::new(l.today, PresetRange::Today),
            Self::new(l.yesterday, PresetRange::Yesterday),
            Self::new(l.tomorrow, PresetRange::Tomorrow),
        ]
    }

    /// What picking this preset selects under `rules`: `Ok(None)` for Custom, `Err(())` when
    /// the preset falls entirely outside what may be picked (its button is disabled).
    #[allow(clippy::result_unit_err)]
    pub fn apply(
        &self,
        today: NaiveDate,
        first_day: Weekday,
        rules: &DateRules,
    ) -> Result<Option<DateRange>, ()> {
        match self.range.resolve(today, first_day) {
            None => Ok(None),
            Some(r) => rules.clamp_range(r.start, r.end).map(Some).ok_or(()),
        }
    }
}

/// The first day of the week containing `d`.
pub fn week_start(d: NaiveDate, first_day: Weekday) -> NaiveDate {
    let back = (d.weekday().num_days_from_sunday() + 7 - first_day.num_days_from_sunday()) % 7;
    d.checked_sub_days(Days::new(u64::from(back))).unwrap_or(d)
}

/// The days of one calendar month grid, whole weeks starting on `first_day`, exactly as
/// Kit's calendar lays them out (4–6 rows; out-of-range months wrap into the next year).
pub fn month_grid(year: i32, month: u32, first_day: Weekday) -> Vec<NaiveDate> {
    let total = i64::from(year) * 12 + i64::from(month) - 1;
    let (Ok(year), Ok(month)) = (
        i32::try_from(total.div_euclid(12)),
        u32::try_from(total.rem_euclid(12) + 1),
    ) else {
        return Vec::new();
    };
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let Some(next) = first.checked_add_months(Months::new(1)) else {
        return Vec::new();
    };
    let start = week_start(first, first_day);
    let count = ((next - start).num_days().max(0) as usize).div_ceil(7) * 7;
    start.iter_days().take(count).collect()
}

/// The grids of `count` consecutive months from `year`/`month`, concatenated in the order
/// Kit's calendar renders their day cells.
pub fn visible_days(year: i32, month: u32, count: usize, first_day: Weekday) -> Vec<NaiveDate> {
    (0..count.max(1))
        .flat_map(|offset| month_grid(year, month + offset as u32, first_day))
        .collect()
}

/// A keyboard move of the calendar cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMove {
    /// Arrow left/right (±1) and up/down (±7).
    Days(i64),
    /// Page up/down; the day is clamped to the target month's length.
    Months(i32),
    /// Shift+Page up/down.
    Years(i32),
    /// Home.
    WeekStart,
    /// End.
    WeekEnd,
}

/// Move the keyboard cursor and land on an allowed day: out-of-bounds targets clamp to
/// `min`/`max`, excluded days are skipped in the direction of travel (Home/End: back into
/// the week), then the other way.
/// If nothing is allowed the cursor stays at `from`.
pub fn move_cursor(
    from: NaiveDate,
    mv: CursorMove,
    rules: &DateRules,
    first_day: Weekday,
) -> NaiveDate {
    let months = |n: i32| {
        if n >= 0 {
            from.checked_add_months(Months::new(n.unsigned_abs()))
        } else {
            from.checked_sub_months(Months::new(n.unsigned_abs()))
        }
    };
    let (target, forward) = match mv {
        CursorMove::Days(n) => (from.checked_add_signed(chrono::Duration::days(n)), n >= 0),
        CursorMove::Months(n) => (months(n), n >= 0),
        CursorMove::Years(n) => (months(n.saturating_mul(12)), n >= 0),
        // Home/End skip excluded days back toward the week they started in.
        CursorMove::WeekStart => (Some(week_start(from, first_day)), true),
        CursorMove::WeekEnd => (
            week_start(from, first_day).checked_add_days(Days::new(6)),
            false,
        ),
    };
    let Some(target) = target else {
        return from;
    };
    let target = rules.clamp(target);
    rules
        .nearest_allowed(target, forward)
        .or_else(|| rules.nearest_allowed(target, !forward))
        .unwrap_or(from)
}

/// Clamp a time into `min..=max` (either bound optional; swapped when reversed).
pub fn clamp_time(t: NaiveTime, min: Option<NaiveTime>, max: Option<NaiveTime>) -> NaiveTime {
    let (min, max) = match (min, max) {
        (Some(a), Some(b)) if a > b => (Some(b), Some(a)),
        other => other,
    };
    let t = min.map_or(t, |m| t.max(m));
    max.map_or(t, |m| t.min(m))
}

/// The parts every picker value is made of. A value type converts to and from these, so one
/// control implements all five pickers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Parts {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
}

/// What a picker edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickerMode {
    Date,
    DateRange,
    Time,
    DateTime,
    DateTimeRange,
}

impl PickerMode {
    pub fn has_calendar(self) -> bool {
        self != Self::Time
    }
    pub fn is_range(self) -> bool {
        matches!(self, Self::DateRange | Self::DateTimeRange)
    }
    pub fn has_time(self) -> bool {
        matches!(self, Self::Time | Self::DateTime | Self::DateTimeRange)
    }
}

/// A value one of the pickers edits.
pub trait PickerValue: Copy + PartialEq + fmt::Debug + 'static {
    const MODE: PickerMode;
    fn to_parts(self) -> Parts;
    /// The value `parts` make once complete; missing times take `default_time`.
    fn from_parts(parts: &Parts, default_time: NaiveTime) -> Option<Self>;
    /// The value moved onto what `rules` allow (dates snapped, ranges clamped); `None` when
    /// nothing of it is pickable.
    fn normalized(self, rules: &DateRules) -> Option<Self>;
}

impl PickerValue for NaiveDate {
    const MODE: PickerMode = PickerMode::Date;
    fn to_parts(self) -> Parts {
        Parts {
            start: Some(self),
            ..Parts::default()
        }
    }
    fn from_parts(parts: &Parts, _: NaiveTime) -> Option<Self> {
        parts.start
    }
    fn normalized(self, rules: &DateRules) -> Option<Self> {
        rules.snap(self)
    }
}

impl PickerValue for DateRange {
    const MODE: PickerMode = PickerMode::DateRange;
    fn to_parts(self) -> Parts {
        Parts {
            start: Some(self.start),
            end: Some(self.end),
            ..Parts::default()
        }
    }
    fn from_parts(parts: &Parts, _: NaiveTime) -> Option<Self> {
        Some(DateRange::new(parts.start?, parts.end?))
    }
    fn normalized(self, rules: &DateRules) -> Option<Self> {
        rules.clamp_range(self.start, self.end)
    }
}

impl PickerValue for NaiveTime {
    const MODE: PickerMode = PickerMode::Time;
    fn to_parts(self) -> Parts {
        Parts {
            start_time: Some(self),
            ..Parts::default()
        }
    }
    fn from_parts(parts: &Parts, _: NaiveTime) -> Option<Self> {
        parts.start_time
    }
    fn normalized(self, _: &DateRules) -> Option<Self> {
        Some(self)
    }
}

impl PickerValue for NaiveDateTime {
    const MODE: PickerMode = PickerMode::DateTime;
    fn to_parts(self) -> Parts {
        Parts {
            start: Some(self.date()),
            start_time: Some(self.time()),
            ..Parts::default()
        }
    }
    fn from_parts(parts: &Parts, default_time: NaiveTime) -> Option<Self> {
        Some(
            parts
                .start?
                .and_time(parts.start_time.unwrap_or(default_time)),
        )
    }
    fn normalized(self, rules: &DateRules) -> Option<Self> {
        Some(rules.snap(self.date())?.and_time(self.time()))
    }
}

impl PickerValue for DateTimeRange {
    const MODE: PickerMode = PickerMode::DateTimeRange;
    fn to_parts(self) -> Parts {
        Parts {
            start: Some(self.start.date()),
            end: Some(self.end.date()),
            start_time: Some(self.start.time()),
            end_time: Some(self.end.time()),
        }
    }
    fn from_parts(parts: &Parts, default_time: NaiveTime) -> Option<Self> {
        Some(DateTimeRange::new(
            parts
                .start?
                .and_time(parts.start_time.unwrap_or(default_time)),
            parts.end?.and_time(parts.end_time.unwrap_or(default_time)),
        ))
    }
    fn normalized(self, rules: &DateRules) -> Option<Self> {
        let r = rules.clamp_range(self.start.date(), self.end.date())?;
        Some(DateTimeRange::new(
            r.start.and_time(self.start.time()),
            r.end.and_time(self.end.time()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
    }

    fn weekends() -> DatePredicate {
        Arc::new(|d: NaiveDate| matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
    }

    #[test]
    fn rules_allow_clamp_and_snap() {
        let rules = DateRules::new(Some(d(2026, 3, 2)), Some(d(2026, 3, 27)))
            .with_exclude(Some(weekends()));
        assert!(rules.allows(d(2026, 3, 2)));
        assert!(!rules.allows(d(2026, 3, 1)), "before min");
        assert!(!rules.allows(d(2026, 3, 7)), "Saturday is excluded");
        assert!(!rules.allows(d(2026, 3, 30)), "after max");
        assert_eq!(rules.clamp(d(2026, 1, 1)), d(2026, 3, 2));
        assert_eq!(rules.clamp(d(2027, 1, 1)), d(2026, 3, 27));
        // Saturday snaps forward to Monday; past the end it clamps then snaps backward.
        assert_eq!(rules.snap(d(2026, 3, 7)), Some(d(2026, 3, 9)));
        assert_eq!(rules.snap(d(2026, 12, 25)), Some(d(2026, 3, 27)));
        assert_eq!(
            rules.nearest_allowed(d(2026, 3, 28), false),
            None,
            "out of bounds"
        );
    }

    #[test]
    fn reversed_bounds_are_swapped_in_release() {
        if cfg!(debug_assertions) {
            let caught = std::panic::catch_unwind(|| {
                DateRules::new(Some(d(2026, 2, 1)), Some(d(2026, 1, 1)))
            });
            assert!(caught.is_err(), "debug builds flag reversed bounds");
            return;
        }
        let r = DateRules::new(Some(d(2026, 2, 1)), Some(d(2026, 1, 1)));
        assert_eq!((r.min, r.max), (Some(d(2026, 1, 1)), Some(d(2026, 2, 1))));
    }

    #[test]
    fn an_all_excluding_predicate_terminates() {
        let rules = DateRules::default().with_exclude(Some(Arc::new(|_| true)));
        assert_eq!(rules.snap(d(2026, 3, 9)), None);
        assert_eq!(
            move_cursor(d(2026, 3, 9), CursorMove::Days(1), &rules, Weekday::Sun),
            d(2026, 3, 9),
            "the cursor stays put"
        );
    }

    #[test]
    fn ranges_are_ordered_and_clamped() {
        let r = DateRange::new(d(2026, 3, 20), d(2026, 3, 10));
        assert_eq!((r.start, r.end), (d(2026, 3, 10), d(2026, 3, 20)));
        assert_eq!(r.days(), 11);
        assert!(r.contains(d(2026, 3, 15)) && !r.contains(d(2026, 3, 21)));

        let rules = DateRules::new(Some(d(2026, 3, 2)), Some(d(2026, 3, 27)))
            .with_exclude(Some(weekends()));
        // Ends move inward onto weekdays; excluded days inside are kept.
        assert_eq!(
            rules.clamp_range(d(2026, 3, 28), d(2026, 2, 1)),
            Some(DateRange::new(d(2026, 3, 2), d(2026, 3, 27)))
        );
        assert_eq!(
            rules.clamp_range(d(2026, 3, 7), d(2026, 3, 8)),
            None,
            "a weekend-only range has nothing pickable"
        );
        assert_eq!(rules.clamp_range(d(2025, 1, 1), d(2025, 2, 1)), None);

        let dt = DateTimeRange::new(
            d(2026, 3, 9).and_hms_opt(18, 0, 0).unwrap_or_default(),
            d(2026, 3, 9).and_hms_opt(9, 0, 0).unwrap_or_default(),
        );
        assert!(dt.start < dt.end);
    }

    #[test]
    fn range_picking_never_reverses() {
        let (a, b, c) = (d(2026, 3, 10), d(2026, 3, 14), d(2026, 3, 5));
        assert_eq!(pick_range((None, None), a), (Some(a), None));
        assert_eq!(pick_range((Some(a), None), b), (Some(a), Some(b)));
        assert_eq!(
            pick_range((Some(a), None), a),
            (Some(a), Some(a)),
            "one day"
        );
        assert_eq!(
            pick_range((Some(a), None), c),
            (Some(c), None),
            "earlier restarts"
        );
        assert_eq!(pick_range((Some(a), Some(b)), c), (Some(c), None));
        assert_eq!(preview_range(a, b), Some(DateRange::new(a, b)));
        assert_eq!(preview_range(a, c), None);
        for (s, e) in [(a, b), (b, a), (c, a)] {
            if let (Some(s), Some(e)) = pick_range(pick_range((None, None), s), e) {
                assert!(s <= e);
            }
        }
    }

    #[test]
    fn presets_resolve_relative_to_today() {
        let today = d(2026, 3, 11); // a Wednesday
        let r = |p: PresetRange, first| p.resolve(today, first).map(|r| (r.start, r.end));
        assert_eq!(r(PresetRange::Today, Weekday::Sun), Some((today, today)));
        assert_eq!(
            r(PresetRange::Yesterday, Weekday::Sun),
            Some((d(2026, 3, 10), d(2026, 3, 10)))
        );
        assert_eq!(
            r(PresetRange::LastDays(7), Weekday::Sun),
            Some((d(2026, 3, 5), today))
        );
        assert_eq!(
            r(PresetRange::NextDays(3), Weekday::Sun),
            Some((today, d(2026, 3, 13)))
        );
        assert_eq!(
            r(PresetRange::ThisWeek, Weekday::Sun),
            Some((d(2026, 3, 8), d(2026, 3, 14)))
        );
        assert_eq!(
            r(PresetRange::ThisWeek, Weekday::Mon),
            Some((d(2026, 3, 9), d(2026, 3, 15)))
        );
        assert_eq!(
            r(PresetRange::LastWeek, Weekday::Mon),
            Some((d(2026, 3, 2), d(2026, 3, 8)))
        );
        assert_eq!(
            r(PresetRange::ThisMonth, Weekday::Sun),
            Some((d(2026, 3, 1), d(2026, 3, 31)))
        );
        assert_eq!(
            r(PresetRange::LastMonth, Weekday::Sun),
            Some((d(2026, 2, 1), d(2026, 2, 28)))
        );
        assert_eq!(
            r(PresetRange::ThisYear, Weekday::Sun),
            Some((d(2026, 1, 1), d(2026, 12, 31)))
        );
        assert_eq!(
            r(
                PresetRange::Fixed(d(2026, 5, 2), d(2026, 5, 1)),
                Weekday::Sun
            ),
            Some((d(2026, 5, 1), d(2026, 5, 2))),
            "fixed ranges are ordered"
        );
        assert_eq!(r(PresetRange::Custom, Weekday::Sun), None);
        assert_eq!(
            PresetRange::LastMonth.resolve(d(2026, 1, 15), Weekday::Sun),
            Some(DateRange::new(d(2025, 12, 1), d(2025, 12, 31))),
            "last month crosses the year"
        );
    }

    #[test]
    fn presets_respect_the_rules() {
        let today = d(2026, 3, 11);
        let rules = DateRules::new(Some(d(2026, 3, 9)), None);
        let last7 = DatePreset::new("Last 7 days", PresetRange::LastDays(7));
        assert_eq!(
            last7.apply(today, Weekday::Sun, &rules),
            Ok(Some(DateRange::new(d(2026, 3, 9), today))),
            "clamped to min"
        );
        let last_month = DatePreset::new("Last month", PresetRange::LastMonth);
        assert_eq!(last_month.apply(today, Weekday::Sun, &rules), Err(()));
        let custom = DatePreset::new("Custom", PresetRange::Custom);
        assert_eq!(custom.apply(today, Weekday::Sun, &rules), Ok(None));
        let labels: Vec<_> = DatePreset::standard_ranges(&DateLocale::en_us())
            .into_iter()
            .map(|p| p.label)
            .collect();
        assert_eq!(
            labels,
            [
                "Today",
                "Last 7 days",
                "Last 30 days",
                "This month",
                "Last month",
                "Custom"
            ]
        );
        assert_eq!(
            DatePreset::standard_days(&DateLocale::de())[0].label,
            "Heute"
        );
    }

    #[test]
    fn month_grid_matches_kit_layout() {
        // August 2025 starts on a Friday: six Sunday-first weeks, from Jul 27.
        let g = month_grid(2025, 8, Weekday::Sun);
        assert_eq!(g.len(), 42);
        assert_eq!(g[0], d(2025, 7, 27));
        assert_eq!(g[41], d(2025, 9, 6));
        // February 2026 starts on a Sunday: exactly four weeks.
        assert_eq!(month_grid(2026, 2, Weekday::Sun).len(), 28);
        let mon = month_grid(2023, 2, Weekday::Mon);
        assert_eq!(mon[0], d(2023, 1, 30));
        // Month overflow wraps into the next year, like Kit's offset months.
        assert_eq!(
            month_grid(2025, 13, Weekday::Sun),
            month_grid(2026, 1, Weekday::Sun)
        );
        let two = visible_days(2025, 12, 2, Weekday::Sun);
        assert_eq!(
            two.len(),
            month_grid(2025, 12, Weekday::Sun).len() + month_grid(2026, 1, Weekday::Sun).len()
        );
    }

    #[test]
    fn cursor_moves_skip_and_clamp() {
        let rules = DateRules::new(Some(d(2026, 3, 2)), Some(d(2026, 3, 31)))
            .with_exclude(Some(weekends()));
        let fri = d(2026, 3, 6);
        let mv = |from, m| move_cursor(from, m, &rules, Weekday::Mon);
        assert_eq!(
            mv(fri, CursorMove::Days(1)),
            d(2026, 3, 9),
            "skips the weekend"
        );
        assert_eq!(mv(d(2026, 3, 9), CursorMove::Days(-1)), fri, "and back");
        assert_eq!(mv(fri, CursorMove::Days(7)), d(2026, 3, 13));
        assert_eq!(
            mv(d(2026, 3, 3), CursorMove::Days(-7)),
            d(2026, 3, 2),
            "clamped to min"
        );
        assert_eq!(
            mv(fri, CursorMove::Months(1)),
            d(2026, 3, 31),
            "clamped to max"
        );
        assert_eq!(mv(d(2026, 3, 4), CursorMove::WeekStart), d(2026, 3, 2));
        assert_eq!(
            mv(d(2026, 3, 4), CursorMove::WeekEnd),
            fri,
            "Sunday is excluded"
        );
        let free = DateRules::default();
        assert_eq!(
            move_cursor(d(2026, 1, 31), CursorMove::Months(1), &free, Weekday::Sun),
            d(2026, 2, 28),
            "day clamped to the shorter month"
        );
        assert_eq!(
            move_cursor(d(2024, 2, 29), CursorMove::Years(1), &free, Weekday::Sun),
            d(2025, 2, 28)
        );
        assert_eq!(
            move_cursor(d(2026, 3, 1), CursorMove::Months(-1), &free, Weekday::Sun),
            d(2026, 2, 1)
        );
    }

    #[test]
    fn month_bounds() {
        let rules = DateRules::new(Some(d(2026, 3, 15)), Some(d(2026, 5, 2)));
        assert!(!rules.month_in_bounds(2026, 2));
        assert!(rules.month_in_bounds(2026, 3));
        assert!(rules.month_in_bounds(2026, 5));
        assert!(!rules.month_in_bounds(2026, 6));
        assert!(!rules.month_in_bounds(2026, 13), "invalid month");
    }

    #[test]
    fn times_clamp() {
        let t = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap_or_default();
        assert_eq!(clamp_time(t(7), Some(t(9)), Some(t(17))), t(9));
        assert_eq!(clamp_time(t(20), Some(t(9)), Some(t(17))), t(17));
        assert_eq!(
            clamp_time(t(12), Some(t(17)), Some(t(9))),
            t(12),
            "swapped bounds"
        );
        assert_eq!(clamp_time(t(3), None, None), t(3));
    }

    #[test]
    fn values_round_trip_through_parts() {
        let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap_or_default();
        let day = d(2026, 3, 9);
        assert_eq!(NaiveDate::from_parts(&day.to_parts(), noon), Some(day));
        let r = DateRange::new(day, d(2026, 3, 12));
        assert_eq!(DateRange::from_parts(&r.to_parts(), noon), Some(r));
        assert_eq!(
            DateRange::from_parts(
                &Parts {
                    start: Some(day),
                    ..Parts::default()
                },
                noon
            ),
            None,
            "an open range is not a value"
        );
        let dt = day.and_hms_opt(9, 30, 0).unwrap_or_default();
        assert_eq!(NaiveDateTime::from_parts(&dt.to_parts(), noon), Some(dt));
        assert_eq!(
            NaiveDateTime::from_parts(&day.to_parts(), noon),
            Some(day.and_time(noon)),
            "a missing time takes the default"
        );
        let dtr = DateTimeRange::new(dt, d(2026, 3, 10).and_time(noon));
        assert_eq!(DateTimeRange::from_parts(&dtr.to_parts(), noon), Some(dtr));
        assert_eq!(NaiveTime::from_parts(&noon.to_parts(), noon), Some(noon));
        let rules = DateRules::new(Some(d(2026, 3, 10)), None);
        assert_eq!(
            dt.normalized(&rules),
            Some(d(2026, 3, 10).and_hms_opt(9, 30, 0).unwrap_or_default()),
            "the time survives clamping"
        );
        assert_eq!(
            r.normalized(&rules),
            Some(DateRange::new(d(2026, 3, 10), d(2026, 3, 12)))
        );
        assert!(PickerMode::DateTimeRange.is_range() && PickerMode::DateTimeRange.has_time());
        assert!(!PickerMode::Time.has_calendar());
    }
}
