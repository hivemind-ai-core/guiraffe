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
    pub color: guiraffe_core::ColorName,
    pub(crate) keys: Vec<f64>,
    pub(crate) decor: Option<Vec<(Option<f64>, KeyShape)>>,
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
        Self {
            name: name.into(),
            color: guiraffe_core::ColorName::Blue,
            keys,
            decor: None,
            children: Vec::new(),
            parent: None,
            group: false,
            collapsed: false,
            muted: false,
            locked: false,
            visible: true,
        }
    }
    pub fn with_color(mut self, color: guiraffe_core::ColorName) -> Self {
        self.color = color;
        self
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
            if let Ok(i) = c
                .keys
                .binary_search_by(|t| t.total_cmp(&if k.t == 0. { 0. } else { k.t }))
            {
                let decor = c
                    .decor
                    .get_or_insert_with(|| vec![(None, KeyShape::Diamond); c.keys.len()]);
                decor[i] = (
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
        self.replace_times(&mut [(from, t)]);
        self.keys.iter().position(|k| *k == t)
    }
    pub(crate) fn replace_times(&mut self, edits: &mut [(f64, f64)]) {
        edits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let replacement = |t: f64| {
            edits
                .binary_search_by(|(from, _)| from.total_cmp(&t))
                .ok()
                .map(|i| edits[i].1)
                .unwrap_or(t)
        };
        if let Some(decor) = &mut self.decor {
            let mut keys: Vec<_> = self
                .keys
                .iter()
                .copied()
                .map(replacement)
                .zip(decor.iter().copied())
                .collect();
            keys.sort_by(|a, b| a.0.total_cmp(&b.0));
            for ((time, meta), (new_time, new_meta)) in self.keys.iter_mut().zip(decor).zip(keys) {
                *time = new_time;
                *meta = new_meta;
            }
        } else {
            for time in &mut self.keys {
                *time = replacement(*time);
            }
            self.keys.sort_unstable_by(f64::total_cmp);
        }
    }
    pub(crate) fn insert_time(&mut self, t: f64) {
        if !t.is_finite() || self.keys.contains(&t) {
            return;
        }
        let i = self.keys.partition_point(|k| *k < t);
        self.keys.insert(i, t);
        if let Some(decor) = &mut self.decor {
            decor.insert(i, (None, KeyShape::Diamond));
        }
    }
    pub(crate) fn restore_time(&mut self, source: &Channel, time: f64) {
        self.insert_time(time);
        if let (Some(from), Some(to)) = (
            source.keys.iter().position(|t| *t == time),
            self.keys.iter().position(|t| *t == time),
        ) {
            if let Some(source_decor) = &source.decor {
                self.decor
                    .get_or_insert_with(|| vec![(None, KeyShape::Diamond); self.keys.len()])[to] =
                    source_decor[from];
            } else if let Some(decor) = &mut self.decor {
                decor[to] = (None, KeyShape::Diamond);
            }
        }
    }
    pub(crate) fn remove_time(&mut self, t: f64) -> bool {
        if let Some(i) = self.keys.iter().position(|k| *k == t) {
            self.keys.remove(i);
            if let Some(decor) = &mut self.decor {
                decor.remove(i);
            }
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
