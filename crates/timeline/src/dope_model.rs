//! Pure channel hierarchy, selection and atomic frame-snapped retiming.
use crate::{
    TimeFormat,
    channels::{Channel, snap_time},
};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelRow {
    pub channel: usize,
    pub depth: usize,
    pub summary: bool,
}
#[derive(Clone, Debug)]
pub struct DopeModel {
    pub channels: Vec<Channel>,
    pub selected: BTreeSet<(usize, u64)>,
    rows: Vec<ChannelRow>,
    summaries: std::collections::BTreeMap<usize, Vec<f64>>,
}
impl DopeModel {
    pub fn new(channels: Vec<Channel>) -> Self {
        fn flatten(input: Vec<Channel>, parent: Option<usize>, out: &mut Vec<Channel>) {
            for mut channel in input {
                let children = std::mem::take(&mut channel.children);
                channel.parent = parent;
                channel.group |= !children.is_empty();
                let index = out.len();
                out.push(channel);
                flatten(children, Some(index), out);
            }
        }
        let mut out = Self {
            channels: Vec::new(),
            selected: BTreeSet::new(),
            rows: Vec::new(),
            summaries: Default::default(),
        };
        flatten(channels, None, &mut out.channels);
        out.rebuild_rows();
        out.rebuild_summaries();
        out
    }
    /// A model over channels already flattened in display order, with `parent` indices set.
    pub(crate) fn from_flat(channels: Vec<Channel>) -> Self {
        let mut out = Self {
            channels,
            selected: BTreeSet::new(),
            rows: Vec::new(),
            summaries: Default::default(),
        };
        out.rebuild_rows();
        out.rebuild_summaries();
        out
    }
    pub fn rows(&self) -> &[ChannelRow] {
        &self.rows
    }
    pub fn rebuild_rows(&mut self) {
        self.rows.clear();
        for (i, c) in self.channels.iter().enumerate() {
            let mut parent = c.parent;
            let mut depth = 0;
            let mut hidden = false;
            while let Some(p) = parent {
                let Some(c) = self.channels.get(p) else { break };
                hidden |= c.collapsed;
                parent = c.parent;
                depth += 1;
            }
            if !hidden {
                self.rows.push(ChannelRow {
                    channel: i,
                    depth,
                    summary: c.group,
                });
            }
        }
    }
    pub fn editable(&self, channel: usize) -> bool {
        let mut at = Some(channel);
        while let Some(i) = at {
            let Some(c) = self.channels.get(i) else {
                return false;
            };
            if c.locked {
                return false;
            }
            at = c.parent;
        }
        true
    }
    pub fn select(&mut self, channel: usize, time: f64, add: bool) {
        if !add {
            self.selected.clear();
        }
        let key = (channel, time.to_bits());
        if add && self.selected.contains(&key) {
            self.selected.remove(&key);
        } else {
            self.selected.insert(key);
        }
    }
    pub fn select_summary(&mut self, channel: usize, time: f64, add: bool) {
        if !add {
            self.selected.clear();
        }
        for (i, c) in self.channels.iter().enumerate() {
            let mut parent = Some(i);
            let mut belongs = false;
            for _ in 0..=self.channels.len() {
                let Some(p) = parent else { break };
                if p == channel {
                    belongs = true;
                    break;
                }
                parent = self.channels.get(p).and_then(|c| c.parent);
            }
            if belongs && c.keys().contains(&time) {
                self.selected.insert((i, time.to_bits()));
            }
        }
    }
    pub fn select_all(&mut self) {
        self.selected = self
            .channels
            .iter()
            .enumerate()
            .flat_map(|(i, c)| c.keys().iter().map(move |t| (i, t.to_bits())))
            .collect();
    }
    pub fn box_select(&mut self, rows: std::ops::RangeInclusive<usize>, a: f64, b: f64, add: bool) {
        if !add {
            self.selected.clear()
        }
        if rows.is_empty() {
            return;
        }
        let Some(rows) = self
            .rows
            .get(*rows.start()..rows.end().saturating_add(1).min(self.rows.len()))
        else {
            return;
        };
        let (min, max) = (a.min(b), a.max(b));
        for row in rows {
            if let Some(channel) = self.channels.get(row.channel).filter(|c| c.visible) {
                let keys = channel.keys();
                let first = keys.partition_point(|time| *time < min);
                let last = keys.partition_point(|time| *time <= max);
                self.selected.extend(
                    keys[first..last]
                        .iter()
                        .map(|time| (row.channel, time.to_bits())),
                );
            }
        }
    }
    pub fn translate(&mut self, delta: f64, format: TimeFormat) -> bool {
        if !delta.is_finite() {
            return false;
        }
        let edits: Vec<_> = self
            .selected
            .iter()
            .filter(|(r, _)| self.editable(*r))
            .map(|(r, t)| (*r, *t, snap_time(f64::from_bits(*t) + delta, format)))
            .collect();
        if edits.is_empty()
            || edits.iter().any(|(_, t, to)| {
                !to.is_finite() || *to < 0. || !(*to - f64::from_bits(*t)).is_finite()
            })
        {
            return false;
        }
        // Reject collisions with unselected keys, and selected keys snapping together.
        let mut destinations = BTreeSet::new();
        for (r, _, to) in &edits {
            let keys = self.channels[*r].keys();
            let index = keys.partition_point(|time| time < to);
            let occupied = keys
                .get(index)
                .is_some_and(|time| *time == *to && !self.selected.contains(&(*r, time.to_bits())));
            if !destinations.insert((*r, to.to_bits())) || occupied {
                return false;
            }
        }
        if edits.iter().all(|(_, t, to)| f64::from_bits(*t) == *to) {
            return false;
        }
        let mut by_channel = std::collections::BTreeMap::<usize, Vec<(f64, f64)>>::new();
        for (r, from, to) in &edits {
            by_channel
                .entry(*r)
                .or_default()
                .push((f64::from_bits(*from), *to));
            self.selected.remove(&(*r, *from));
        }
        for (r, mut edits) in by_channel {
            self.channels[r].replace_times(&mut edits);
        }
        for (r, _, to) in edits {
            self.selected.insert((r, to.to_bits()));
        }
        self.rebuild_summaries();
        true
    }
    pub fn delete_selected(&mut self) -> bool {
        let selected: Vec<_> = self
            .selected
            .iter()
            .copied()
            .filter(|(r, _)| self.editable(*r))
            .collect();
        let mut changed = false;
        for (r, t) in selected {
            changed |= self.channels[r].remove_time(f64::from_bits(t));
            self.selected.remove(&(r, t));
        }
        if changed {
            self.rebuild_summaries();
        }
        changed
    }
    pub fn duplicate(&mut self, delta: f64, format: TimeFormat) -> bool {
        let mut copy = self.clone();
        if !copy.translate(delta, format) {
            return false;
        }
        // Translation must not replace originals or any existing time.
        if copy
            .selected
            .iter()
            .filter(|(r, _)| self.editable(*r))
            .any(|(r, t)| self.channels[*r].keys().contains(&f64::from_bits(*t)))
        {
            return false;
        }
        for (r, t) in &self.selected {
            if self.editable(*r) {
                copy.channels[*r].restore_time(&self.channels[*r], f64::from_bits(*t));
            }
        }
        copy.rebuild_summaries();
        *self = copy;
        true
    }
    /// Summary diamonds represent an aggregate, not a value segment: they never
    /// borrow interpolation/hold metadata from a group's own key indices.
    pub fn key_display(
        &self,
        channel: usize,
        summary: bool,
        index: usize,
    ) -> (crate::KeyShape, Option<f64>) {
        let Some(c) = self.channels.get(channel).filter(|_| !summary) else {
            return (crate::KeyShape::Diamond, None);
        };
        let Some(metadata) = &c.decor else {
            return (crate::KeyShape::Diamond, None);
        };
        let decor = metadata
            .get(index)
            .copied()
            .unwrap_or((None, crate::KeyShape::Diamond));
        let Some(next_index) = index.checked_add(1) else {
            return (decor.1, None);
        };
        let hold = metadata
            .get(next_index)
            .filter(|next| decor.0.is_some() && decor.0 == next.0)
            .and_then(|_| c.keys().get(next_index))
            .copied();
        (decor.1, hold)
    }
    fn descends_from(&self, mut child: usize, parent: usize) -> bool {
        for _ in 0..=self.channels.len() {
            if child == parent {
                return true;
            }
            let Some(next) = self.channels.get(child).and_then(|c| c.parent) else {
                return false;
            };
            child = next;
        }
        false
    }
    pub fn summary_selected(&self, channel: usize, time: f64) -> bool {
        self.selected
            .iter()
            .any(|(child, t)| *t == time.to_bits() && self.descends_from(*child, channel))
    }
    pub fn summary_keys(&self, channel: usize) -> &[f64] {
        self.summaries
            .get(&channel)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub(crate) fn rebuild_summaries(&mut self) {
        self.summaries = self
            .channels
            .iter()
            .enumerate()
            .filter(|(_, c)| c.group)
            .map(|(i, _)| (i, self.compute_summary(i)))
            .collect();
    }
    fn compute_summary(&self, channel: usize) -> Vec<f64> {
        let mut result = self
            .channels
            .get(channel)
            .map(|c| c.keys().to_vec())
            .unwrap_or_default();
        for (i, c) in self.channels.iter().enumerate() {
            if i == channel {
                continue;
            }
            let mut parent = c.parent;
            while let Some(p) = parent {
                if p == channel {
                    result.extend_from_slice(c.keys());
                    break;
                }
                parent = self.channels.get(p).and_then(|c| c.parent);
            }
        }
        result.sort_by(f64::total_cmp);
        result.dedup();
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn box_selection_ignores_rows_removed_by_host() {
        let mut model = DopeModel::new(vec![Channel::new("Track", vec![1.])]);
        model.select_all();
        model.channels.clear();
        model.box_select(0..=0, 0., 2., false);
        assert!(model.selected.is_empty());
    }

    proptest::proptest! {
        #[test]
        fn retime_preserves_sorted_unique_keys_and_rejects_collisions_atomically(
            raw in proptest::collection::vec((0_i16..100, proptest::bool::ANY), 0..100),
            delta in -10_i16..10,
        ) {
            let times: BTreeSet<_> = raw.iter().map(|(time, _)| *time).collect();
            let selected: BTreeSet<_> = raw.iter().filter_map(|(time, on)| on.then_some(*time)).collect();
            let fixed: BTreeSet<_> = times.difference(&selected).copied().collect();
            let moved: BTreeSet<_> = selected.iter().map(|time| time + delta).collect();
            let valid = delta != 0 && !moved.is_empty() && moved.is_disjoint(&fixed)
                && moved.iter().all(|time| *time >= 0);
            let mut model = DopeModel::new(vec![Channel::new(
                "Track", times.iter().copied().map(f64::from).collect(),
            )]);
            for time in &selected {
                model.select(0, f64::from(*time), true);
            }
            proptest::prop_assert_eq!(model.translate(f64::from(delta), TimeFormat::Frames(1.)), valid);
            let expected: Vec<_> = if valid { fixed.union(&moved).copied().collect() }
                else { times.into_iter().collect() };
            let expected: Vec<_> = expected.into_iter().map(f64::from).collect();
            proptest::prop_assert_eq!(model.channels[0].keys(), expected.as_slice());
            let active = if valid { moved } else { selected };
            let active: BTreeSet<_> = active.into_iter().map(|time| (0, f64::from(time).to_bits())).collect();
            proptest::prop_assert_eq!(model.selected, active);
        }

        #[test]
        fn box_selection_matches_naive_scan(
            keys in proptest::collection::vec(-100_i16..100, 0..100),
            a in -120_i16..120,
            b in -120_i16..120,
        ) {
            let channel = Channel::new("Track", keys.iter().copied().map(f64::from).collect());
            let mut model = DopeModel::new(vec![channel]);
            model.box_select(0..=usize::MAX, f64::from(a), f64::from(b), false);
            let expected: BTreeSet<_> = keys.iter()
                .filter(|&&time| (a.min(b)..=a.max(b)).contains(&time))
                .map(|&time| (0, f64::from(time).to_bits()))
                .collect();
            proptest::prop_assert_eq!(model.selected, expected);
        }
    }

    #[test]
    fn hierarchy_and_atomic_selection_moves() {
        let mut m = DopeModel::new(vec![Channel::group(
            "Object",
            vec![
                Channel::new("x", vec![0., 1., 2.]),
                Channel::new("y", vec![0., 1.]),
            ],
        )]);
        assert_eq!(m.rows().len(), 3);
        m.channels[0].collapsed = true;
        m.rebuild_rows();
        assert_eq!(m.rows().len(), 1);
        m.channels[0].collapsed = false;
        m.rebuild_rows();
        m.box_select(1..=2, 0., 1., false);
        assert_eq!(m.selected.len(), 4);
        assert!(
            !m.translate(1., TimeFormat::Frames(24.)),
            "collision rejects entire move"
        );
        assert!(m.translate(3., TimeFormat::Frames(24.)));
        assert_eq!(m.channels[1].keys(), &[2., 3., 4.]);
        m.channels[0].locked = true;
        assert!(!m.translate(1., TimeFormat::Seconds));
        assert!(!m.delete_selected());
        m.channels[0].locked = false;
        assert!(m.duplicate(10., TimeFormat::Seconds));
        assert_eq!(m.selected.len(), 4);
        assert!(m.delete_selected());
        assert_eq!(m.channels[1].keys(), &[2., 3., 4.]);
    }
}

#[cfg(test)]
mod summary_visual_tests {
    use super::*;
    #[test]
    fn summaries_do_not_index_group_metadata() {
        let mut group = Channel::with_keyframes(
            "group",
            vec![
                crate::Key::new(0.5, 1., crate::Interp::Constant),
                crate::Key::new(1., 1., crate::Interp::Constant),
            ],
        );
        group.group = true;
        group.children.push(Channel::with_keyframes(
            "child",
            vec![crate::Key::new(0., 0., crate::Interp::Bezier)],
        ));
        let mut m = DopeModel::new(vec![group]);
        assert_eq!(m.summary_keys(0), &[0., 0.5, 1.]);
        assert_eq!(m.key_display(0, true, 0), (crate::KeyShape::Diamond, None));
        assert_eq!(
            m.key_display(0, false, 0),
            (crate::KeyShape::Square, Some(1.))
        );
        assert_eq!(m.key_display(1, false, 0), (crate::KeyShape::Circle, None));
        m.select(1, 0., false);
        assert!(m.summary_selected(0, 0.));
    }
}
