//! Pure channel data, key shapes and frame snapping. No GPUI operations.
use crate::TimeFormat;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyShape {
    #[default]
    Diamond,
    Circle,
    Square,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub name: String,
    pub(crate) keys: Vec<f64>,
    pub(crate) decor: Vec<(Option<f64>, KeyShape)>,
    pub children: Vec<Channel>,
    pub parent: Option<usize>,
    pub group: bool,
    pub collapsed: bool,
    pub muted: bool,
    pub locked: bool,
    pub visible: bool,
}
impl Default for Channel {
    fn default() -> Self {
        Self::new("", Vec::new())
    }
}
impl Channel {
    pub fn new(name: impl Into<String>, mut keys: Vec<f64>) -> Self {
        keys.retain(|t| t.is_finite());
        for t in &mut keys {
            if *t == 0. {
                *t = 0.;
            }
        }
        keys.sort_by(f64::total_cmp);
        keys.dedup();
        let decor = vec![(None, KeyShape::Diamond); keys.len()];
        Self {
            name: name.into(),
            keys,
            decor,
            children: Vec::new(),
            parent: None,
            group: false,
            collapsed: false,
            muted: false,
            locked: false,
            visible: true,
        }
    }
    pub fn group(name: impl Into<String>, children: Vec<Channel>) -> Self {
        Self {
            children,
            group: true,
            ..Self::new(name, vec![])
        }
    }
    pub fn with_keyframes(name: impl Into<String>, keys: Vec<crate::Key>) -> Self {
        let mut c = Self::new(name, keys.iter().map(|k| k.t).collect());
        for k in keys {
            if let Some(i) = c.keys.iter().position(|t| *t == k.t) {
                c.decor[i] = (
                    Some(k.v),
                    match k.interp {
                        crate::Interp::Constant => KeyShape::Square,
                        crate::Interp::Linear => KeyShape::Diamond,
                        crate::Interp::Bezier => KeyShape::Circle,
                    },
                );
            }
        }
        c
    }
    pub fn keys(&self) -> &[f64] {
        &self.keys
    }
    pub fn retime(&mut self, i: usize, t: f64) -> Option<usize> {
        let from = *self.keys.get(i)?;
        if !t.is_finite() || from == t || self.keys.contains(&t) {
            return None;
        }
        self.replace_times(&[(from, t)]);
        self.keys.iter().position(|k| *k == t)
    }
    pub(crate) fn replace_times(&mut self, edits: &[(f64, f64)]) {
        let mut keys: Vec<_> = self
            .keys
            .iter()
            .copied()
            .zip(self.decor.iter().copied())
            .map(|(t, d)| {
                (
                    edits
                        .iter()
                        .find(|(from, _)| *from == t)
                        .map(|(_, to)| *to)
                        .unwrap_or(t),
                    d,
                )
            })
            .collect();
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.keys = keys.iter().map(|p| p.0).collect();
        self.decor = keys.into_iter().map(|p| p.1).collect();
    }
    pub(crate) fn insert_time(&mut self, t: f64) {
        if !t.is_finite() || self.keys.contains(&t) {
            return;
        }
        let i = self.keys.partition_point(|k| *k < t);
        self.keys.insert(i, t);
        self.decor.insert(i, (None, KeyShape::Diamond));
    }
    pub(crate) fn restore_time(&mut self, source: &Channel, time: f64) {
        self.insert_time(time);
        if let (Some(from), Some(to)) = (
            source.keys.iter().position(|t| *t == time),
            self.keys.iter().position(|t| *t == time),
        ) {
            self.decor[to] = source.decor[from];
        }
    }
    pub(crate) fn remove_time(&mut self, t: f64) -> bool {
        if let Some(i) = self.keys.iter().position(|k| *k == t) {
            self.keys.remove(i);
            self.decor.remove(i);
            true
        } else {
            false
        }
    }
}
pub fn snap_time(t: f64, format: TimeFormat) -> f64 {
    match format {
        TimeFormat::Frames(fps) if fps.is_finite() && fps > 0. => (t * fps).round() / fps,
        _ => t,
    }
}
