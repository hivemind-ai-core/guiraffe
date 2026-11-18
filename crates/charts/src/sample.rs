/// Largest Triangle Three Buckets. Writes increasing original indices into reusable scratch.
/// For two or more output points the first and last input points are preserved.
pub fn lttb(
    len: usize,
    threshold: usize,
    point: impl Fn(usize) -> (f64, f64),
    out: &mut Vec<usize>,
) {
    out.clear();
    if threshold == 0 || len == 0 {
        return;
    }
    if threshold >= len {
        out.extend(0..len);
        return;
    }
    out.push(0);
    if threshold == 1 {
        return;
    }
    let buckets = threshold - 2;
    let width = (len - 2) as f64 / buckets.max(1) as f64;
    let start = |bucket: usize| ((bucket as f64 * width) as usize + 1).min(len - 1);
    let mut anchor = point(0);
    for bucket in 0..buckets {
        let average = start(bucket + 1)..(start(bucket + 2)).max(start(bucket + 1) + 1);
        let count = average.len() as f64;
        let (x, y) = average.fold((0., 0.), |(x, y), index| {
            let p = point(index);
            (x + p.0 / count, y + p.1 / count)
        });
        let mut best = (start(bucket), -1.);
        for index in start(bucket)..start(bucket + 1) {
            let p = point(index);
            let area =
                ((anchor.0 - x) * (p.1 - anchor.1) - (anchor.0 - p.0) * (y - anchor.1)).abs();
            if area > best.1 {
                best = (index, area);
            }
        }
        out.push(best.0);
        anchor = point(best.0);
    }
    out.push(len - 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn indices_are_bounded_ordered_and_keep_endpoints(
            data in proptest::collection::vec(-1e6f64..1e6, 0..300),
            budget in 0usize..350
        ) {
            let mut indices = vec![usize::MAX];
            lttb(data.len(), budget, |i| (i as f64, data[i]), &mut indices);
            prop_assert_eq!(indices.len(), budget.min(data.len()));
            prop_assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
            if !indices.is_empty() {
                prop_assert_eq!(indices[0], 0);
            }
            if indices.len() > 1 {
                prop_assert_eq!(indices.last(), Some(&(data.len() - 1)));
            }
        }
    }

    #[test]
    fn selects_an_isolated_spike() {
        let mut indices = Vec::new();
        lttb(
            1000,
            30,
            |i| (i as f64, if i == 420 { 100. } else { 0. }),
            &mut indices,
        );
        assert!(indices.contains(&420));
    }
}
