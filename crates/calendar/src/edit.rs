use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::EventId;

pub const SNAP_MINUTES: i64 = 15;

pub fn snap_minutes(delta: i64) -> i64 {
    let delta = i128::from(delta);
    let snap = i128::from(SNAP_MINUTES);
    let magnitude = (delta.abs() + snap / 2) / snap * snap;
    let signed = if delta.is_negative() {
        -magnitude
    } else {
        magnitude
    };
    signed.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    Start,
    End,
}

pub fn move_timed(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    delta: i64,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let delta = Duration::minutes(snap_minutes(delta));
    Some((
        start.checked_add_signed(delta)?,
        end.checked_add_signed(delta)?,
    ))
}

pub fn resize_timed(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    edge: ResizeEdge,
    delta: i64,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let delta = Duration::minutes(snap_minutes(delta));
    let minimum = Duration::minutes(SNAP_MINUTES);
    let result = match edge {
        ResizeEdge::Start => {
            let next = start.checked_add_signed(delta)?.min(end - minimum);
            (next, end)
        }
        ResizeEdge::End => {
            let next = end.checked_add_signed(delta)?.max(start + minimum);
            (start, next)
        }
    };
    Some(result)
}

pub fn move_all_day(
    start: NaiveDate,
    end_exclusive: NaiveDate,
    delta: i64,
) -> Option<(NaiveDate, NaiveDate)> {
    let delta = Duration::days(delta);
    Some((
        start.checked_add_signed(delta)?,
        end_exclusive.checked_add_signed(delta)?,
    ))
}

pub fn resize_all_day(
    start: NaiveDate,
    end_exclusive: NaiveDate,
    edge: ResizeEdge,
    delta: i64,
) -> Option<(NaiveDate, NaiveDate)> {
    let delta = Duration::days(delta);
    match edge {
        ResizeEdge::Start => Some((
            start
                .checked_add_signed(delta)?
                .min(end_exclusive - Duration::days(1)),
            end_exclusive,
        )),
        ResizeEdge::End => Some((
            start,
            end_exclusive
                .checked_add_signed(delta)?
                .max(start + Duration::days(1)),
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedSlice {
    pub id: EventId,
    pub start_second: i32,
    pub end_second: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositionedEvent {
    pub id: EventId,
    pub start_second: i32,
    pub end_second: i32,
    pub lane: usize,
    pub lanes: usize,
    pub continued_before: bool,
    pub continued_after: bool,
}

pub fn layout_overlaps(items: &mut [TimedSlice]) -> Vec<PositionedEvent> {
    items.sort_by(|left, right| {
        (
            left.start_second,
            std::cmp::Reverse(left.end_second),
            left.id,
        )
            .cmp(&(
                right.start_second,
                std::cmp::Reverse(right.end_second),
                right.id,
            ))
    });
    let mut output: Vec<PositionedEvent> = Vec::with_capacity(items.len());
    let mut lane_ends = Vec::<i32>::new();
    let mut cluster_end = i32::MIN;
    let mut cluster_start_index = 0usize;
    let mut clusters = Vec::<(usize, usize, usize)>::new();
    for item in items.iter() {
        if !output.is_empty() && item.start_second >= cluster_end {
            clusters.push((cluster_start_index, output.len(), lane_ends.len()));
            cluster_start_index = output.len();
            lane_ends.clear();
            cluster_end = i32::MIN;
        }
        let lane = lane_ends
            .iter()
            .position(|end| *end <= item.start_second)
            .unwrap_or_else(|| {
                lane_ends.push(item.start_second);
                lane_ends.len().saturating_sub(1)
            });
        if let Some(end) = lane_ends.get_mut(lane) {
            *end = item.end_second;
        }
        cluster_end = cluster_end.max(item.end_second);
        output.push(PositionedEvent {
            id: item.id,
            start_second: item.start_second,
            end_second: item.end_second,
            lane,
            lanes: 1,
            continued_before: item.start_second == 0,
            continued_after: item.end_second == 86_400,
        });
    }
    if !output.is_empty() {
        clusters.push((cluster_start_index, output.len(), lane_ends.len()));
    }
    for (start, end, lanes) in clusters {
        for positioned in output.get_mut(start..end).into_iter().flatten() {
            positioned.lanes = lanes.max(1);
        }
    }
    output
}

pub fn visible_hour_range(
    scroll_minutes: i32,
    viewport_minutes: i32,
    overscan_hours: i32,
) -> (i32, i32) {
    let scroll = scroll_minutes.clamp(0, 1_440);
    let viewport = viewport_minutes.max(0);
    let overscan = overscan_hours.max(0).saturating_mul(60);
    (
        (scroll - overscan).max(0),
        (scroll + viewport + overscan).min(1_440),
    )
}

pub fn chip_capacity(
    available_height: f32,
    header_height: f32,
    chip_height: f32,
    overflow_height: f32,
) -> usize {
    if !available_height.is_finite()
        || !header_height.is_finite()
        || !chip_height.is_finite()
        || chip_height <= 0.0
    {
        return 0;
    }
    let available = (available_height - header_height).max(0.0);
    let potential = (available / chip_height).floor() as usize;
    if potential == 0 {
        return 0;
    }
    let with_overflow = ((available - overflow_height.max(0.0)) / chip_height)
        .floor()
        .max(0.0) as usize;
    potential.min(with_overflow.max(1))
}

/// How many of `items` chips a month cell shows, and how many go behind `+N more`.
///
/// The overflow affordance is only reserved when the items do not all fit, so a
/// cell never hides an item just to make room for a `+0` button.
pub fn month_chip_plan(
    items: usize,
    available_height: f32,
    header_height: f32,
    chip_height: f32,
    overflow_height: f32,
) -> (usize, usize) {
    let all = chip_capacity(available_height, header_height, chip_height, 0.0);
    if items <= all {
        return (items, 0);
    }
    let shown = chip_capacity(
        available_height,
        header_height,
        chip_height,
        overflow_height,
    )
    .min(items);
    (shown, items - shown)
}

/// Limit a day column to `max_lanes` lanes. Events in lanes beyond the limit are
/// dropped from `positions` and counted, so a dense cluster renders a bounded
/// number of blocks plus one count affordance instead of sub-pixel children.
pub fn collapse_lanes(positions: &mut Vec<PositionedEvent>, max_lanes: usize) -> usize {
    let max_lanes = max_lanes.max(1);
    let before = positions.len();
    positions.retain(|position| position.lane < max_lanes);
    for position in positions.iter_mut() {
        position.lanes = position.lanes.min(max_lanes);
    }
    before - positions.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn snap_handles_negative_half_steps_without_drift() {
        assert_eq!(snap_minutes(0), 0);
        assert_eq!(snap_minutes(7), 0);
        assert_eq!(snap_minutes(8), 15);
        assert_eq!(snap_minutes(-7), 0);
        assert_eq!(snap_minutes(-8), -15);
        assert_eq!(snap_minutes(i64::MIN + 7), i64::MIN + 8);
    }

    #[test]
    fn moves_and_resizes_preserve_duration_and_minimum() {
        let start = Utc
            .with_ymd_and_hms(2026, 11, 21, 9, 0, 0)
            .single()
            .unwrap_or(DateTime::UNIX_EPOCH);
        let end = Utc
            .with_ymd_and_hms(2026, 11, 21, 10, 30, 0)
            .single()
            .unwrap_or(DateTime::UNIX_EPOCH);
        let (moved_start, moved_end) = move_timed(start, end, -31).unwrap_or((start, end));
        assert_eq!(moved_end - moved_start, Duration::minutes(90));
        let (_, resized_end) =
            resize_timed(start, end, ResizeEdge::End, -600).unwrap_or((start, end));
        assert_eq!(resized_end - start, Duration::minutes(15));
        let (resized_start, _) =
            resize_timed(start, end, ResizeEdge::Start, 600).unwrap_or((start, end));
        assert_eq!(end - resized_start, Duration::minutes(15));
    }

    #[test]
    fn overlapping_events_share_cluster_width() {
        let mut items = [
            TimedSlice {
                id: EventId(2),
                start_second: 3600,
                end_second: 7200,
            },
            TimedSlice {
                id: EventId(1),
                start_second: 0,
                end_second: 5400,
            },
            TimedSlice {
                id: EventId(3),
                start_second: 7200,
                end_second: 9000,
            },
            TimedSlice {
                id: EventId(4),
                start_second: 10_800,
                end_second: 12_600,
            },
        ];
        let output = layout_overlaps(&mut items);
        assert_eq!(output[0].lanes, 2);
        assert_eq!(output[1].lanes, 2);
        assert_eq!(
            output[2].lanes, 1,
            "touching half-open intervals start a new cluster"
        );
        assert_eq!(output[2].lane, 0);
        assert_eq!(output[3].lanes, 1);
    }

    #[test]
    fn hour_virtualization_and_capacity_clamp_edges() {
        assert_eq!(visible_hour_range(480, 600, 1), (420, 1_140));
        assert_eq!(visible_hour_range(-50, 100, 2), (0, 220));
        assert_eq!(chip_capacity(10.0, 10.0, 2.0, 0.0), 0);
        assert_eq!(chip_capacity(100.0, 20.0, 20.0, 0.0), 4);
    }

    #[test]
    fn month_chip_plan_reserves_overflow_only_when_needed() {
        // Four chip rows fit; reserving one row for "+N" leaves three.
        assert_eq!(month_chip_plan(0, 100.0, 20.0, 20.0, 20.0), (0, 0));
        assert_eq!(month_chip_plan(4, 100.0, 20.0, 20.0, 20.0), (4, 0));
        assert_eq!(month_chip_plan(5, 100.0, 20.0, 20.0, 20.0), (3, 2));
        assert_eq!(month_chip_plan(9, 100.0, 20.0, 20.0, 0.0), (4, 5));
        // Zero-size and non-finite cells hide everything behind the count.
        assert_eq!(month_chip_plan(3, 0.0, 20.0, 20.0, 20.0), (0, 3));
        assert_eq!(month_chip_plan(3, f32::NAN, 20.0, 20.0, 20.0), (0, 3));
    }

    #[test]
    fn dense_clusters_collapse_to_a_bounded_lane_count() {
        let mut items: Vec<TimedSlice> = (0..10_000)
            .map(|index| TimedSlice {
                id: EventId(index),
                start_second: 3_600,
                end_second: 7_200,
            })
            .collect();
        let mut positions = layout_overlaps(&mut items);
        assert_eq!(positions.len(), 10_000);
        let hidden = collapse_lanes(&mut positions, 4);
        assert_eq!(hidden, 9_996);
        assert_eq!(positions.len(), 4);
        assert!(positions.iter().all(|p| p.lanes == 4 && p.lane < 4));
        let mut single = vec![PositionedEvent {
            id: EventId(1),
            start_second: 0,
            end_second: 60,
            lane: 0,
            lanes: 1,
            continued_before: false,
            continued_after: false,
        }];
        assert_eq!(
            collapse_lanes(&mut single, 0),
            0,
            "at least one lane is kept"
        );
    }

    proptest::proptest! {
        #[test]
        fn snapping_is_always_on_grid(delta in -100_000i64..100_000) {
            proptest::prop_assert_eq!(snap_minutes(delta) % SNAP_MINUTES, 0);
        }

        #[test]
        fn overlap_lanes_are_always_nonempty(
            start in 0i32..80_000,
            duration in 1i32..6_400,
        ) {
            let mut items = [TimedSlice { id: EventId(1), start_second: start, end_second: start.saturating_add(duration) }];
            let output = layout_overlaps(&mut items);
            proptest::prop_assert_eq!(output.len(), 1);
            proptest::prop_assert!(output[0].lanes > 0);
        }
    }
}
