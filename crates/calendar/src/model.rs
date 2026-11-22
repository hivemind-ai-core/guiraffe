use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use chrono::offset::LocalResult;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Offset, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use guiraffe_core::ColorName;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EventId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarSource {
    pub id: SourceId,
    pub name: Arc<str>,
    pub color: ColorName,
    pub visible: bool,
}

impl CalendarSource {
    pub fn new(id: SourceId, name: impl Into<Arc<str>>, color: ColorName) -> Self {
        Self {
            id,
            name: name.into(),
            color,
            visible: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ItemTime {
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    AllDay {
        start: NaiveDate,
        end_exclusive: NaiveDate,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarItem {
    pub id: EventId,
    pub source: SourceId,
    pub title: Arc<str>,
    pub time: ItemTime,
    pub color: Option<ColorName>,
    pub locked: bool,
    pub icon: Option<Arc<str>>,
}

impl CalendarItem {
    pub fn timed(
        id: EventId,
        source: SourceId,
        title: impl Into<Arc<str>>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Self, CalendarError> {
        let time = ItemTime::Timed { start, end };
        validate_time(&time)?;
        Ok(Self {
            id,
            source,
            title: title.into(),
            time,
            color: None,
            locked: false,
            icon: None,
        })
    }

    pub fn all_day(
        id: EventId,
        source: SourceId,
        title: impl Into<Arc<str>>,
        start: NaiveDate,
        end_exclusive: NaiveDate,
    ) -> Result<Self, CalendarError> {
        let time = ItemTime::AllDay {
            start,
            end_exclusive,
        };
        validate_time(&time)?;
        Ok(Self {
            id,
            source,
            title: title.into(),
            time,
            color: None,
            locked: false,
            icon: None,
        })
    }

    pub const fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }

    pub const fn locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    pub fn icon(mut self, icon: impl Into<Arc<str>>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

fn validate_time(time: &ItemTime) -> Result<(), CalendarError> {
    let invalid = match time {
        ItemTime::Timed { start, end } => end <= start,
        ItemTime::AllDay {
            start,
            end_exclusive,
        } => end_exclusive <= start,
    };
    if invalid {
        Err(CalendarError::InvalidRange)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarError {
    InvalidRange,
    DuplicateEvent(EventId),
    DuplicateSource(SourceId),
    MissingSource(SourceId),
}

impl fmt::Display for CalendarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange => f.write_str("calendar interval must end after it starts"),
            Self::DuplicateEvent(id) => write!(f, "duplicate calendar event id {}", id.0),
            Self::DuplicateSource(id) => write!(f, "duplicate calendar source id {}", id.0),
            Self::MissingSource(id) => {
                write!(f, "calendar event refers to missing source {}", id.0)
            }
        }
    }
}

impl std::error::Error for CalendarError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarZone {
    #[default]
    Utc,
    Fixed(FixedOffset),
    Iana(Tz),
}

impl CalendarZone {
    pub fn local(self, instant: DateTime<Utc>) -> DateTime<FixedOffset> {
        match self {
            Self::Utc => instant.with_timezone(&Utc.fix()),
            Self::Fixed(offset) => instant.with_timezone(&offset),
            Self::Iana(zone) => instant.with_timezone(&zone).fixed_offset(),
        }
    }

    pub fn from_local(&self, local: chrono::NaiveDateTime) -> LocalResult<DateTime<Utc>> {
        match self {
            Self::Utc => Utc
                .from_local_datetime(&local)
                .map(|value| value.with_timezone(&Utc)),
            Self::Fixed(offset) => offset
                .from_local_datetime(&local)
                .map(|value| value.with_timezone(&Utc)),
            Self::Iana(zone) => zone
                .from_local_datetime(&local)
                .map(|value| value.with_timezone(&Utc)),
        }
    }

    pub fn single_from_local(self, local: chrono::NaiveDateTime) -> Option<DateTime<Utc>> {
        match self.from_local(local) {
            LocalResult::Single(value) => Some(value),
            LocalResult::Ambiguous(earlier, _) => Some(earlier),
            LocalResult::None => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CalendarModel {
    items: Arc<[Arc<CalendarItem>]>,
    sources: Arc<[CalendarSource]>,
    revision: u64,
}

impl CalendarModel {
    pub fn new(
        items: impl IntoIterator<Item = Arc<CalendarItem>>,
        sources: impl IntoIterator<Item = CalendarSource>,
    ) -> Result<Self, CalendarError> {
        let items: Arc<[Arc<CalendarItem>]> = items.into_iter().collect();
        let sources: Arc<[CalendarSource]> = sources.into_iter().collect();
        validate_model(&items, &sources)?;
        Ok(Self {
            items,
            sources,
            revision: 0,
        })
    }

    pub fn sources(&self) -> &[CalendarSource] {
        &self.sources
    }

    pub fn items(&self) -> &[Arc<CalendarItem>] {
        &self.items
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn source(&self, id: SourceId) -> Option<&CalendarSource> {
        self.sources.iter().find(|source| source.id == id)
    }

    pub fn item(&self, id: EventId) -> Option<&Arc<CalendarItem>> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn set_items(
        &mut self,
        items: impl IntoIterator<Item = Arc<CalendarItem>>,
    ) -> Result<u64, CalendarError> {
        let items: Arc<[Arc<CalendarItem>]> = items.into_iter().collect();
        validate_model(&items, &self.sources)?;
        self.items = items;
        self.revision = self.revision.wrapping_add(1);
        Ok(self.revision)
    }

    pub fn set_source_visible(&mut self, id: SourceId, visible: bool) -> Option<u64> {
        let index = self.sources.iter().position(|source| source.id == id)?;
        if self.sources.get(index)?.visible == visible {
            return Some(self.revision);
        }
        let mut sources = self.sources.to_vec();
        sources.get_mut(index)?.visible = visible;
        self.sources = sources.into();
        self.revision = self.revision.wrapping_add(1);
        Some(self.revision)
    }

    pub fn index(&self, zone: CalendarZone) -> CalendarIndex {
        CalendarIndex::new(self, zone)
    }
}

fn validate_model(
    items: &[Arc<CalendarItem>],
    sources: &[CalendarSource],
) -> Result<(), CalendarError> {
    let mut event_ids = BTreeSet::new();
    let mut source_ids = BTreeSet::new();
    for source in sources {
        if !source_ids.insert(source.id) {
            return Err(CalendarError::DuplicateSource(source.id));
        }
    }
    for item in items {
        validate_time(&item.time)?;
        if !event_ids.insert(item.id) {
            return Err(CalendarError::DuplicateEvent(item.id));
        }
        if !source_ids.contains(&item.source) {
            return Err(CalendarError::MissingSource(item.source));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct CalendarIndex {
    by_day: Arc<BTreeMap<NaiveDate, Vec<Arc<CalendarItem>>>>,
    revision: u64,
    zone: CalendarZone,
}

impl CalendarIndex {
    fn new(model: &CalendarModel, zone: CalendarZone) -> Self {
        let mut by_day = BTreeMap::<NaiveDate, Vec<Arc<CalendarItem>>>::new();
        for item in model
            .items
            .iter()
            .filter(|item| visible_source(model, item.source))
        {
            match &item.time {
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                } => {
                    insert_days(&mut by_day, item, *start, *end_exclusive);
                }
                ItemTime::Timed { start, end } => {
                    let local_start = zone.local(*start);
                    let local_end = zone.local(*end);
                    let mut end_date = local_end.date_naive();
                    if local_end.time() == chrono::NaiveTime::MIN {
                        end_date = end_date.pred_opt().unwrap_or(end_date);
                    }
                    if let Some(next) = end_date.succ_opt() {
                        insert_days(&mut by_day, item, local_start.date_naive(), next);
                    }
                }
            }
        }
        for items in by_day.values_mut() {
            items.sort_by(|left, right| item_order(left, right));
        }
        Self {
            by_day: Arc::new(by_day),
            revision: model.revision,
            zone,
        }
    }

    pub fn day(&self, date: NaiveDate) -> &[Arc<CalendarItem>] {
        self.by_day.get(&date).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn count(&self, date: NaiveDate) -> usize {
        self.day(date).len()
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn zone(&self) -> CalendarZone {
        self.zone
    }
}

fn visible_source(model: &CalendarModel, id: SourceId) -> bool {
    model.source(id).is_some_and(|source| source.visible)
}

fn insert_days(
    by_day: &mut BTreeMap<NaiveDate, Vec<Arc<CalendarItem>>>,
    item: &Arc<CalendarItem>,
    start: NaiveDate,
    end_exclusive: NaiveDate,
) {
    let mut date = start;
    while date < end_exclusive {
        by_day.entry(date).or_default().push(Arc::clone(item));
        let Some(next) = date.succ_opt() else { break };
        date = next;
    }
}

fn item_order(left: &CalendarItem, right: &CalendarItem) -> std::cmp::Ordering {
    let key = |item: &CalendarItem| match &item.time {
        ItemTime::AllDay { .. } => (0, DateTime::<Utc>::UNIX_EPOCH, -Duration::days(1), item.id),
        ItemTime::Timed { start, end } => (1, *start, *start - *end, item.id),
    };
    key(left).cmp(&key(right))
}

pub fn seconds_in_display_day(
    item: &CalendarItem,
    date: NaiveDate,
    zone: CalendarZone,
) -> Option<(i32, i32)> {
    time_seconds_in_display_day(&item.time, date, zone)
}

pub fn time_seconds_in_display_day(
    time: &ItemTime,
    date: NaiveDate,
    zone: CalendarZone,
) -> Option<(i32, i32)> {
    let ItemTime::Timed { start, end } = time else {
        return None;
    };
    let local_start = zone.local(*start);
    let local_end = zone.local(*end);
    if local_start.date_naive() > date
        || local_end.date_naive() < date
        || (local_end.date_naive() == date && local_end.time() == chrono::NaiveTime::MIN)
    {
        return None;
    }
    let start_second = if local_start.date_naive() < date {
        0
    } else {
        (local_start.hour() * 3600 + local_start.minute() * 60 + local_start.second()) as i32
    };
    let end_second = if local_end.date_naive() > date {
        86_400
    } else {
        (local_end.hour() * 3600 + local_end.minute() * 60 + local_end.second()) as i32
    };
    Some((
        start_second.max(0),
        end_second.max(start_second + 1).min(86_400),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn source(id: u64) -> CalendarSource {
        CalendarSource::new(SourceId(id), "Work", ColorName::Blue)
    }

    #[test]
    fn duplicate_and_missing_identities_are_rejected() {
        let item = Arc::new(
            CalendarItem::all_day(
                EventId(1),
                SourceId(1),
                "A",
                NaiveDate::MIN,
                NaiveDate::MIN.succ_opt().unwrap_or(NaiveDate::MIN),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        );
        assert!(matches!(
            CalendarModel::new([item.clone(), item], [source(1)]),
            Err(CalendarError::DuplicateEvent(_))
        ));
        let other = Arc::new(
            CalendarItem::all_day(
                EventId(2),
                SourceId(9),
                "B",
                NaiveDate::MIN,
                NaiveDate::MIN.succ_opt().unwrap_or(NaiveDate::MIN),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        );
        assert!(matches!(
            CalendarModel::new([other], [source(1)]),
            Err(CalendarError::MissingSource(SourceId(9)))
        ));
    }

    #[test]
    fn source_visibility_filters_lookup_and_counts() -> Result<(), CalendarError> {
        let date = NaiveDate::from_ymd_opt(2026, 11, 21).ok_or(CalendarError::InvalidRange)?;
        let end = date.succ_opt().ok_or(CalendarError::InvalidRange)?;
        let one = Arc::new(CalendarItem::all_day(
            EventId(1),
            SourceId(1),
            "One",
            date,
            end,
        )?);
        let two = Arc::new(CalendarItem::all_day(
            EventId(2),
            SourceId(2),
            "Two",
            date,
            end,
        )?);
        let mut model = CalendarModel::new([one, two], [source(1), source(2)])?;
        assert_eq!(model.index(CalendarZone::Utc).count(date), 2);
        model.set_source_visible(SourceId(1), false);
        let index = model.index(CalendarZone::Utc);
        assert_eq!(index.count(date), 1);
        assert_eq!(index.day(date)[0].id, EventId(2));
        Ok(())
    }

    #[test]
    fn timed_items_cross_local_midnight_without_touching_exclusive_end() -> Result<(), CalendarError>
    {
        let first = NaiveDate::from_ymd_opt(2026, 11, 21).ok_or(CalendarError::InvalidRange)?;
        let next = first.succ_opt().ok_or(CalendarError::InvalidRange)?;
        let start = Utc
            .with_ymd_and_hms(2026, 11, 22, 4, 30, 0)
            .earliest()
            .ok_or(CalendarError::InvalidRange)?;
        let end = Utc
            .with_ymd_and_hms(2026, 11, 23, 5, 0, 0)
            .earliest()
            .ok_or(CalendarError::InvalidRange)?;
        let item = Arc::new(CalendarItem::timed(
            EventId(1),
            SourceId(1),
            "Late",
            start,
            end,
        )?);
        let model = CalendarModel::new([item], [source(1)])?;
        let index = model.index(CalendarZone::Iana(chrono_tz::America::New_York));
        assert_eq!(index.count(first), 1);
        assert_eq!(index.count(next), 1);
        Ok(())
    }

    #[test]
    fn named_zone_distinguishes_dst_gap_and_fold() -> Result<(), CalendarError> {
        let zone = CalendarZone::Iana(chrono_tz::America::New_York);
        let gap = NaiveDate::from_ymd_opt(2026, 3, 8)
            .and_then(|date| date.and_hms_opt(2, 30, 0))
            .ok_or(CalendarError::InvalidRange)?;
        assert!(zone.single_from_local(gap).is_none());
        let fold = NaiveDate::from_ymd_opt(2026, 11, 1)
            .and_then(|date| date.and_hms_opt(1, 30, 0))
            .ok_or(CalendarError::InvalidRange)?;
        assert!(matches!(
            zone.from_local(fold),
            LocalResult::Ambiguous(_, _)
        ));
        assert!(
            zone.single_from_local(fold).is_some(),
            "editing uses the earlier fold deterministically"
        );
        Ok(())
    }

    proptest::proptest! {
        #[test]
        fn random_operations_match_a_naive_reference_model(
            operations in proptest::collection::vec(
                (0u8..4, 0u64..12, 0i64..40, 1i64..5, proptest::bool::ANY),
                1..50,
            ),
        ) {
            let base = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap_or(NaiveDate::MIN);
            let mut naive = std::collections::BTreeMap::<u64, (i64, i64, u64)>::new();
            let mut hidden = std::collections::BTreeSet::<u64>::new();
            let mut model = CalendarModel::new([], [source(1), source(2)])
                .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
            for (kind, id, day, days, second) in operations {
                let source_id = if second { 2 } else { 1 };
                match kind {
                    0 | 1 => {
                        naive.insert(id, (day, days, source_id));
                    }
                    2 => {
                        naive.remove(&id);
                    }
                    _ => {
                        let visible = hidden.remove(&source_id);
                        if !visible {
                            hidden.insert(source_id);
                        }
                        model.set_source_visible(SourceId(source_id), visible);
                    }
                }
                let mut items = Vec::new();
                for (&id, &(day, days, source_id)) in &naive {
                    let item = CalendarItem::all_day(
                        EventId(id),
                        SourceId(source_id),
                        "item",
                        base + Duration::days(day),
                        base + Duration::days(day + days),
                    )
                    .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
                    items.push(Arc::new(item));
                }
                model
                    .set_items(items)
                    .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
                let index = model.index(CalendarZone::Utc);
                for day in 0..45 {
                    let expected: Vec<EventId> = naive
                        .iter()
                        .filter(|(_, (start, days, source_id))| {
                            *start <= day && day < start + days && !hidden.contains(source_id)
                        })
                        .map(|(id, _)| EventId(*id))
                        .collect();
                    let got: Vec<EventId> = index
                        .day(base + Duration::days(day))
                        .iter()
                        .map(|item| item.id)
                        .collect();
                    proptest::prop_assert_eq!(got, expected);
                }
            }
        }

        #[test]
        fn arbitrary_valid_events_never_leave_empty_bucket(day in 0i32..20_000) {
            let Some(date) = DateTime::<Utc>::UNIX_EPOCH.date_naive().checked_add_signed(Duration::days(i64::from(day))) else { return Ok(()); };
            let Some(end) = date.succ_opt() else { return Ok(()); };
            let item = Arc::new(CalendarItem::all_day(EventId(1), SourceId(1), "A", date, end).map_err(std::io::Error::other)?);
            let model = CalendarModel::new([item], [source(1)]).map_err(std::io::Error::other)?;
            proptest::prop_assert_eq!(model.index(CalendarZone::Utc).count(date), 1);
        }
    }
}
