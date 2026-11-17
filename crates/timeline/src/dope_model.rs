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
        for r in rows {
            if let Some(row) = self.rows.get(r) {
                let c = &self.channels[row.channel];
                if c.visible {
                    for t in c
                        .keys()
                        .iter()
                        .filter(|t| **t >= a.min(b) && **t <= a.max(b))
                    {
                        self.selected.insert((row.channel, t.to_bits()));
                    }
                }
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
            if !destinations.insert((*r, to.to_bits()))
                || self.channels[*r]
                    .keys()
                    .iter()
                    .any(|t| *t == *to && !self.selected.contains(&(*r, t.to_bits())))
            {
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
        for (r, edits) in by_channel {
            self.channels[r].replace_times(&edits);
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
    pub fn summary_keys(&self, channel: usize) -> &[f64] {
        self.summaries
            .get(&channel)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    fn rebuild_summaries(&mut self) {
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
