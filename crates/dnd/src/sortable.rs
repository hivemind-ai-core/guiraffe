//! Reorder and transfer operations shared by list, grid and tab presentations.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reorder {
    pub original_indices: Vec<usize>,
    pub insertion_index: usize,
}

fn selection(indices: &[usize], len: usize) -> Option<Vec<usize>> {
    let mut indices = indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    (!indices.is_empty() && indices.last().is_some_and(|index| *index < len)).then_some(indices)
}

fn extract<T>(source: &mut Vec<T>, indices: &[usize]) -> Vec<T> {
    let mut index = 0;
    let mut selected = indices.iter().copied().peekable();
    let (remaining, moved) = source.drain(..).partition(|_| {
        let keep = selected.next_if_eq(&index).is_none();
        index += 1;
        keep
    });
    *source = remaining;
    moved
}

/// Move selected items, preserving their relative order. The insertion gap
/// is measured in the original list and may be at its end.
pub fn reorder<T>(items: &mut Vec<T>, indices: &[usize], gap: usize) -> Option<Reorder> {
    let indices = selection(indices, items.len())?;
    if gap > items.len() {
        return None;
    }
    let insertion_index = gap - indices.partition_point(|index| *index < gap);
    if indices
        .iter()
        .copied()
        .eq(insertion_index..insertion_index + indices.len())
    {
        return None;
    }
    let moved = extract(items, &indices);
    items.splice(insertion_index..insertion_index, moved);
    Some(Reorder {
        original_indices: indices,
        insertion_index,
    })
}

/// Transfer selected items between collections. The gap is measured in the destination.
pub fn transfer<T>(
    source: &mut Vec<T>,
    destination: &mut Vec<T>,
    indices: &[usize],
    gap: usize,
) -> Option<Reorder> {
    let indices = selection(indices, source.len())?;
    if gap > destination.len() {
        return None;
    }
    destination.splice(gap..gap, extract(source, &indices));
    Some(Reorder {
        original_indices: indices,
        insertion_index: gap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_small_reorders_preserve_selected_and_unselected_order() {
        for len in 1..=8 {
            for mask in 1..(1 << len) {
                let original: Vec<_> = (0..len).collect();
                let selected: Vec<_> = (0..len).filter(|i| mask & (1 << i) != 0).collect();
                let remaining: Vec<_> = (0..len).filter(|i| mask & (1 << i) == 0).collect();
                for gap in 0..=len {
                    let mut items = original.clone();
                    let index = remaining.iter().filter(|&&i| i < gap).count();
                    let expected: Vec<_> = remaining[..index]
                        .iter()
                        .chain(&selected)
                        .chain(&remaining[index..])
                        .copied()
                        .collect();
                    let event = reorder(&mut items, &selected, gap);
                    assert_eq!(items, expected);
                    assert_eq!(event.is_some(), items != original);
                }
            }
        }
    }

    #[test]
    fn transfer_validates_before_mutating() {
        let mut source = vec![1, 2, 3, 4];
        let mut destination = vec![5, 6];
        assert!(transfer(&mut source, &mut destination, &[4], 1).is_none());
        assert!(transfer(&mut source, &mut destination, &[0], 3).is_none());
        assert_eq!(source, [1, 2, 3, 4]);
        assert_eq!(destination, [5, 6]);
        assert!(transfer(&mut source, &mut destination, &[3, 1, 3], 1).is_some());
        assert_eq!(source, [1, 3]);
        assert_eq!(destination, [5, 2, 4, 6]);
    }
}
