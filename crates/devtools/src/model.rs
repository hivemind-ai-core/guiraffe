use serde_json::Value;
use std::{collections::VecDeque, time::Duration};

/// Bounded rolling samples. Storage is allocated once; recording never grows it.
#[derive(Debug)]
pub struct FrameLog {
    samples: VecDeque<FrameSample>,
    capacity: usize,
}

/// Actual GPUI draw work, excluding display presentation and waiting for vsync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameSample {
    pub total: Duration,
    pub layout: Duration,
    pub prepaint: Duration,
    pub paint: Duration,
}

impl FrameLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub fn record(&mut self, sample: FrameSample) {
        if self.capacity == 0 {
            return;
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
    pub fn samples(&self) -> &VecDeque<FrameSample> {
        &self.samples
    }
    /// Nearest-rank percentile, defined for 0..=100 (higher inputs clamp).
    /// Sorting happens only when displaying the panel, never on the record path.
    pub fn percentile(&self, percentile: usize) -> Option<Duration> {
        let mut values: Vec<_> = self.samples.iter().map(|s| s.total).collect();
        if values.is_empty() {
            return None;
        }
        values.sort_unstable();
        let rank = (values.len() * percentile.min(100)).div_ceil(100).max(1);
        values.get(rank - 1).copied()
    }
    /// Exclusive buckets: <=4, <=8, <=16, <=33, <=50 and >50 milliseconds.
    pub fn histogram(&self) -> [usize; 6] {
        let mut buckets = [0; 6];
        for sample in &self.samples {
            let index = [4, 8, 16, 33, 50]
                .iter()
                .position(|ms| sample.total <= Duration::from_millis(*ms))
                .unwrap_or(5);
            if let Some(bucket) = buckets.get_mut(index) {
                *bucket += 1;
            }
        }
        buckets
    }
}

/// A changed JSON pointer. Missing values represent additions/deletions; JSON null stays null.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDiff {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// Structural object diff; arrays are atomic to keep index shifts unambiguous.
pub fn diff(before: &Value, after: &Value) -> Vec<StateDiff> {
    fn walk(path: &str, before: Option<&Value>, after: Option<&Value>, out: &mut Vec<StateDiff>) {
        if before == after {
            return;
        }
        if let (Some(Value::Object(a)), Some(Value::Object(b))) = (before, after) {
            for key in a.keys().chain(b.keys().filter(|key| !a.contains_key(*key))) {
                let key_path = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                walk(&key_path, a.get(key), b.get(key), out);
            }
        } else {
            out.push(StateDiff {
                path: path.into(),
                before: before.cloned(),
                after: after.cloned(),
            });
        }
    }
    let mut out = Vec::new();
    walk("", Some(before), Some(after), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rolling_histogram_percentiles_and_empty_log() {
        let mut log = FrameLog::new(100);
        assert_eq!(log.percentile(50), None);
        for ms in 1..=101 {
            log.record(FrameSample {
                total: Duration::from_millis(ms),
                ..Default::default()
            });
        }
        assert_eq!(log.samples().len(), 100);
        assert_eq!(log.percentile(50), Some(Duration::from_millis(51)));
        assert_eq!(log.percentile(99), Some(Duration::from_millis(100)));
        assert_eq!(log.percentile(0), Some(Duration::from_millis(2)));
        assert_eq!(log.histogram(), [3, 4, 8, 17, 17, 51]);
        let mut zero = FrameLog::new(0);
        zero.record(FrameSample::default());
        assert!(zero.samples().is_empty());
    }
    #[test]
    fn diff_preserves_null_and_escapes_json_pointers() {
        let changes = diff(
            &json!({"a/b": {"~": 1}, "removed": null}),
            &json!({"a/b": {"~": 2}, "added": null}),
        );
        assert!(changes.iter().any(|d| d.path == "/a~1b/~0"
            && d.before == Some(json!(1))
            && d.after == Some(json!(2))));
        assert!(
            changes.iter().any(|d| d.path == "/removed"
                && d.before == Some(Value::Null)
                && d.after.is_none())
        );
        assert!(
            changes
                .iter()
                .any(|d| d.path == "/added" && d.before.is_none() && d.after == Some(Value::Null))
        );
        assert!(diff(&json!([1]), &json!([1])).is_empty());
        assert_eq!(diff(&json!([1]), &json!([1, 2])).len(), 1);
    }
}
