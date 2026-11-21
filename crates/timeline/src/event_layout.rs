//! Data validation and viewport projection for a multi-track event timeline.

use std::{collections::HashMap, fmt, ops::Range};

use guiraffe_core::ColorName;

/// A named lane in display order.
#[derive(Clone, Debug)]
pub struct EventTrack {
    pub id: u64,
    pub label: String,
    pub color: Option<ColorName>,
}

/// One event and its default textual content.
#[derive(Clone, Debug)]
pub struct TimelineEvent {
    pub id: u64,
    pub track: u64,
    pub position: i64,
    pub label: String,
    pub content: Option<String>,
    pub tags: Vec<String>,
    pub color: Option<ColorName>,
}

/// The relationship indicated by a cross-track connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Branch,
    Merge,
}

/// A directed cross-track connector.
#[derive(Clone, Copy, Debug)]
pub struct EventLink {
    pub source: u64,
    pub destination: u64,
    pub kind: LinkKind,
    pub color: Option<ColorName>,
}

/// An invalid model input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventModelError {
    DuplicateTrack(u64),
    DuplicateEvent(u64),
    UnknownTrack(u64),
    UnknownEvent(u64),
    InvalidLink(EventLinkIds),
}

/// Endpoint IDs for an invalid connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventLinkIds(pub u64, pub u64);

impl fmt::Display for EventModelError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "invalid event timeline: {self:?}")
    }
}

impl std::error::Error for EventModelError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub event: usize,
    pub slot: usize,
    pub track: usize,
    pub row: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RoutedLink {
    pub from: Placement,
    pub to: Placement,
    pub link: EventLink,
}

pub(crate) const DOT_OFFSET: f64 = 0.4;

impl RoutedLink {
    /// In (chronology, track) coordinates, measured in slots and lanes.
    pub fn points(self) -> [(f64, f64); 4] {
        let start = self.from.slot as f64 + DOT_OFFSET;
        let end = self.to.slot as f64 + DOT_OFFSET;
        let from = self.from.track as f64;
        let to = self.to.track as f64;
        let gutter = if start == end {
            self.from.slot as f64 + 0.08
        } else {
            self.from.slot as f64 + 0.97
        };
        [(start, from), (gutter, from), (gutter, to), (end, to)]
    }
}

/// Validated immutable input and its reusable indexes.
#[derive(Debug)]
pub struct EventModel {
    pub(crate) tracks: Vec<EventTrack>,
    pub(crate) events: Vec<TimelineEvent>,
    pub(crate) lanes: Vec<Vec<Placement>>,
    pub(crate) slots: usize,
    placements: HashMap<u64, Placement>,
    links: Vec<RoutedLink>,
    max_end: Vec<usize>,
    incident: HashMap<u64, [usize; 2]>,
}

impl EventModel {
    /// Build shared slot alignment. Equal positions retain caller order in each track.
    pub fn new(
        tracks: Vec<EventTrack>,
        events: Vec<TimelineEvent>,
        links: Vec<EventLink>,
    ) -> Result<Self, EventModelError> {
        let mut track_index = HashMap::with_capacity(tracks.len());
        for (index, track) in tracks.iter().enumerate() {
            if track_index.insert(track.id, index).is_some() {
                return Err(EventModelError::DuplicateTrack(track.id));
            }
        }
        let mut order = Vec::with_capacity(events.len());
        for (index, event) in events.iter().enumerate() {
            let track = track_index
                .get(&event.track)
                .copied()
                .ok_or(EventModelError::UnknownTrack(event.track))?;
            order.push((event.position, track, index));
        }
        order.sort_unstable();
        let mut lanes = vec![Vec::new(); tracks.len()];
        let mut placements = HashMap::with_capacity(events.len());
        let mut base = 0;
        for group in order.chunk_by(|left, right| left.0 == right.0) {
            let mut width = 0;
            for stack in group.chunk_by(|left, right| left.1 == right.1) {
                width = width.max(stack.len());
                for (offset, &(_, track, event)) in stack.iter().enumerate() {
                    if let (Some(lane), Some(item)) = (lanes.get_mut(track), events.get(event)) {
                        let placement = Placement {
                            event,
                            slot: base + offset,
                            track,
                            row: lane.len(),
                        };
                        if placements.insert(item.id, placement).is_some() {
                            return Err(EventModelError::DuplicateEvent(item.id));
                        }
                        lane.push(placement);
                    }
                }
            }
            base += width;
        }
        let mut routed = Vec::with_capacity(links.len());
        let mut incident = HashMap::new();
        for link in links {
            let from = placements
                .get(&link.source)
                .copied()
                .ok_or(EventModelError::UnknownEvent(link.source))?;
            let to = placements
                .get(&link.destination)
                .copied()
                .ok_or(EventModelError::UnknownEvent(link.destination))?;
            if from.track == to.track || from.slot > to.slot {
                return Err(EventModelError::InvalidLink(EventLinkIds(
                    link.source,
                    link.destination,
                )));
            }
            let kind = usize::from(link.kind == LinkKind::Merge);
            for id in [link.source, link.destination] {
                if let Some(count) = incident.entry(id).or_insert([0; 2]).get_mut(kind) {
                    *count += 1;
                }
            }
            routed.push(RoutedLink { from, to, link });
        }
        routed.sort_unstable_by_key(|route| {
            (
                route.from.slot,
                route.to.slot,
                route.link.source,
                route.link.destination,
            )
        });
        let leaf_count = routed.len().next_power_of_two();
        let mut max_end = vec![0; leaf_count * 2];
        for (end, route) in max_end.iter_mut().skip(leaf_count).zip(&routed) {
            *end = route.to.slot;
        }
        for index in (1..leaf_count).rev() {
            let end = max_end
                .get(index * 2)
                .copied()
                .unwrap_or_default()
                .max(max_end.get(index * 2 + 1).copied().unwrap_or_default());
            if let Some(slot) = max_end.get_mut(index) {
                *slot = end;
            }
        }
        Ok(Self {
            tracks,
            events,
            lanes,
            slots: base,
            placements,
            links: routed,
            max_end,
            incident,
        })
    }

    pub(crate) fn locate(&self, id: u64) -> Option<Placement> {
        self.placements.get(&id).copied()
    }

    pub(crate) fn relations(&self, id: u64) -> [usize; 2] {
        self.incident.get(&id).copied().unwrap_or_default()
    }

    pub(crate) fn links(&self, slots: Range<usize>, mut visit: impl FnMut(RoutedLink)) {
        if slots.is_empty() {
            return;
        }
        let end = self
            .links
            .partition_point(|route| route.from.slot < slots.end);
        self.visit_links(1, 0..self.max_end.len() / 2, slots.start, end, &mut visit);
    }

    fn visit_links(
        &self,
        node: usize,
        rows: Range<usize>,
        start: usize,
        end: usize,
        visit: &mut impl FnMut(RoutedLink),
    ) {
        if rows.start >= end || self.max_end.get(node).is_none_or(|last| *last < start) {
            return;
        }
        if rows.len() == 1 {
            if let Some(route) = self.links.get(rows.start) {
                visit(*route);
            }
            return;
        }
        let middle = rows.start + rows.len() / 2;
        self.visit_links(node * 2, rows.start..middle, start, end, visit);
        self.visit_links(node * 2 + 1, middle..rows.end, start, end, visit);
    }
}

/// Clip one orthogonal connector segment to the visible chronology interval.
pub(crate) fn clip_segment(
    from: (f64, f64),
    to: (f64, f64),
    visible: Range<f64>,
) -> Option<[(f64, f64); 2]> {
    let (first, last) = (from.0.min(to.0), from.0.max(to.0));
    if last < visible.start || first > visible.end || visible.is_empty() {
        return None;
    }
    let start = (from.0.clamp(visible.start, visible.end), from.1);
    let end = (to.0.clamp(visible.start, visible.end), to.1);
    (start != end).then_some([start, end])
}

pub(crate) fn visible_range(offset: f64, viewport: f64, pitch: f64, count: usize) -> Range<usize> {
    let start = (offset / pitch).floor() as usize;
    let end = ((offset + viewport) / pitch).ceil() as usize;
    start.min(count)..end.max(start).min(count)
}

pub(crate) fn visible_events(lane: &[Placement], slots: Range<usize>) -> &[Placement] {
    let start = lane.partition_point(|item| item.slot < slots.start);
    let end = lane.partition_point(|item| item.slot < slots.end);
    lane.get(start..end).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn stacking_and_windows_match_reference(
            rows in prop::collection::vec((0u64..4, -10i64..10), 0..150),
            start in 0usize..150,
            length in 0usize..50,
        ) {
            let tracks = (0..4).map(|id| EventTrack { id, label: id.to_string(), color: None }).collect();
            let events = rows.iter().enumerate().map(|(id, &(track, position))| TimelineEvent {
                id: id as u64, track, position, label: id.to_string(), content: None, tags: vec![], color: None,
            }).collect();
            let model = EventModel::new(tracks, events, vec![]).unwrap_or_else(|error| panic!("{error}"));
            let mut expected = vec![Vec::new(); 4];
            let mut base = 0;
            for position in -10..10 {
                let mut width = 0;
                for (track, lane) in expected.iter_mut().enumerate() {
                    let matching = rows.iter().enumerate().filter(|(_, row)| **row == (track as u64, position));
                    let mut count = 0;
                    for (index, _) in matching {
                        lane.push((index, base + count));
                        count += 1;
                    }
                    width = width.max(count);
                }
                base += width;
            }
            prop_assert_eq!(model.slots, base);
            let range = visible_range(start as f64, length as f64, 1.0, base);
            for (lane, expected) in model.lanes.iter().zip(expected) {
                prop_assert_eq!(lane.iter().map(|p| (p.event, p.slot)).collect::<Vec<_>>(), expected.clone());
                prop_assert_eq!(visible_events(lane, range.clone()).iter().map(|p| (p.event, p.slot)).collect::<Vec<_>>(),
                    expected.into_iter().filter(|(_, slot)| range.contains(slot)).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn same_slot_crossing_is_not_projected_onto_viewport_edge() {
        assert_eq!(clip_segment((1.125, 0.0), (1.125, 2.0), 1.2..2.0), None);
        assert_eq!(
            clip_segment((1.125, 2.0), (3.5, 2.0), 1.2..2.0),
            Some([(1.2, 2.0), (2.0, 2.0)])
        );
    }

    proptest! {
        #[test]
        fn interval_index_retains_connectors_with_offscreen_endpoints(
            routes in prop::collection::vec((0u64..64, 0u64..64), 0..200),
            start in 0usize..64,
            length in 1usize..32,
        ) {
            let tracks = (0..2).map(|id| EventTrack {
                id, label: id.to_string(), color: None,
            }).collect();
            let events = (0..128).map(|id| TimelineEvent {
                id, track: id % 2, position: (id / 2) as i64, label: String::new(),
                content: None, tags: vec![], color: None,
            }).collect();
            let links: Vec<_> = routes.into_iter().map(|(a, b)| EventLink {
                source: a.min(b) * 2, destination: a.max(b) * 2 + 1,
                kind: LinkKind::Branch, color: None,
            }).collect();
            let model = EventModel::new(tracks, events, links.clone())
                .unwrap_or_else(|error| panic!("{error}"));
            let end = start + length;
            let mut expected: Vec<_> = links.into_iter().filter(|link| {
                link.source / 2 < end as u64 && link.destination / 2 >= start as u64
            }).map(|link| (link.source, link.destination)).collect();
            expected.sort_unstable();
            let mut found = Vec::new();
            model.links(start..end, |route| found.push((route.link.source, route.link.destination)));
            found.sort_unstable();
            prop_assert_eq!(found, expected);
        }
    }

    proptest! {
        #[test]
        fn route_attaches_and_crosses_through_slot_gutter(from in 0usize..100, distance in 0usize..100, from_track in 0usize..15, lane_delta in 1usize..10) {
            let to = from + distance;
            let to_track = from_track + lane_delta;
            let route = RoutedLink {
                from: Placement { event: 0, slot: from, track: from_track, row: 0 },
                to: Placement { event: 1, slot: to, track: to_track, row: 0 },
                link: EventLink { source: 0, destination: 1, kind: LinkKind::Branch, color: None },
            };
            let [start, turn, exit, end] = route.points();
            prop_assert_eq!(start, (from as f64 + DOT_OFFSET, from_track as f64));
            prop_assert_eq!(end, (to as f64 + DOT_OFFSET, to_track as f64));
            prop_assert_eq!(turn.0, exit.0);
            prop_assert_eq!(turn.1, start.1);
            prop_assert_eq!(exit.1, end.1);
            let turn_is_in_gutter = if distance == 0 {
                turn.0 < start.0
            } else {
                turn.0 > start.0 && turn.0 < (from + 1) as f64
            };
            prop_assert!(turn_is_in_gutter);
        }
    }
}
