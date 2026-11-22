use std::collections::HashMap;

/// Fixed-default metrics with sparse Fenwick prefix corrections.
pub struct Axis {
    len: usize,
    default: f64,
    sizes: HashMap<usize, f64>,
    sums: HashMap<usize, f64>,
}

impl Axis {
    pub fn new(len: usize, default: f64) -> Self {
        let default = if default.is_finite() {
            default.clamp(1., 4096.)
        } else {
            24.
        };
        Self {
            len,
            default,
            sizes: HashMap::new(),
            sums: HashMap::new(),
        }
    }

    pub fn size(&self, index: usize) -> f64 {
        self.sizes.get(&index).copied().unwrap_or(self.default)
    }

    pub fn resize(&mut self, index: usize, size: f64) {
        if index >= self.len || !size.is_finite() {
            return;
        }
        let size = size.clamp(1., 4096.);
        let delta = size - self.size(index);
        if size == self.default {
            self.sizes.remove(&index);
        } else {
            self.sizes.insert(index, size);
        }
        let mut i = index + 1;
        while i <= self.len {
            let sum = self.sums.entry(i).or_default();
            *sum += delta;
            if *sum == 0. {
                self.sums.remove(&i);
            }
            let Some(next) = i.checked_add(i.isolate_lowest_one()) else {
                break;
            };
            i = next;
        }
    }

    pub fn offset(&self, index: usize) -> f64 {
        let mut i = index.min(self.len);
        let mut offset = i as f64 * self.default;
        while i > 0 {
            offset += self.sums.get(&i).copied().unwrap_or_default();
            i &= i - 1;
        }
        offset
    }

    /// Index containing the offset, or the length when at/past the far edge.
    pub fn index(&self, offset: f64) -> usize {
        if self.len == 0 || offset < 0. {
            return 0;
        }
        let mut bit = 1 << (usize::BITS - 1 - self.len.leading_zeros());
        let mut index = 0usize;
        let mut delta = 0.;
        while bit > 0 {
            if let Some(next) = index.checked_add(bit)
                && next <= self.len
            {
                let correction = self.sums.get(&next).copied().unwrap_or_default();
                if next as f64 * self.default + delta + correction <= offset {
                    delta += correction;
                    index = next;
                }
            }
            bit >>= 1;
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn sparse_metrics_match_dense(
            changes in proptest::collection::vec((0usize..64, 1u16..400), 0..100)
        ) {
            let mut axis = Axis::new(64, 20.);
            let mut dense = [20.; 64];
            for (index, value) in changes {
                axis.resize(index, f64::from(value));
                dense[index] = f64::from(value);
            }
            let mut offset = 0.;
            for (index, value) in dense.into_iter().enumerate() {
                prop_assert_eq!(axis.offset(index), offset);
                prop_assert_eq!(axis.index(offset), index);
                prop_assert_eq!(axis.index(offset + value / 2.), index);
                offset += value;
            }
            prop_assert_eq!(axis.index(offset), dense.len());
        }
    }
}
