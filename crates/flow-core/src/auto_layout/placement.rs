use super::{LayoutCancellation, LayoutError, LayoutOptions, Link, Unit};
use crate::{Point, Rect};
use std::collections::{BTreeMap, VecDeque};

pub(super) fn grid(units: &[Unit], columns: usize, spacing: Point) -> Vec<Point> {
    let columns = if columns == 0 {
        (units.len() as f64).sqrt().ceil() as usize
    } else {
        columns
    };
    let mut order: Vec<_> = (0..units.len()).collect();
    order.sort_by_key(|&id| (units[id].order, id));
    let mut positions = vec![Point::default(); units.len()];
    let mut y = 0.;
    for row in order.chunks(columns.max(1)) {
        let mut x = 0.;
        let mut height: f32 = 0.;
        for &id in row {
            positions[id] = Point::new(x, y);
            x += units[id].bounds.size.x + spacing.x;
            height = height.max(units[id].bounds.size.y);
        }
        y += height + spacing.y;
    }
    positions
}

pub(super) fn force(
    units: &[Unit],
    links: &[Link],
    options: &LayoutOptions,
    mut seed: u64,
    iterations: usize,
    cancel: &LayoutCancellation,
) -> Result<Vec<Point>, LayoutError> {
    let mut positions = grid(units, 0, options.spacing);
    let distance = units
        .iter()
        .map(|u| u.bounds.size.x.max(u.bounds.size.y))
        .fold(1., f32::max)
        + options.spacing.x.max(options.spacing.y);
    if !distance.is_finite() {
        return Err(LayoutError::Infeasible);
    }
    let next = |seed: &mut u64| {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (*seed >> 40) as f32 / 16777216. - 0.5
    };
    for (pos, unit) in positions.iter_mut().zip(units) {
        if unit.fixed || options.affected.is_some() {
            *pos = unit.bounds.origin;
        } else {
            *pos = *pos + Point::new(next(&mut seed), next(&mut seed)) * (distance * 0.25);
        }
    }
    let mut forces = vec![Point::default(); units.len()];
    for step in 0..iterations {
        cancel.check()?;
        forces.fill(Point::default());
        for link in links {
            cancel.check()?;
            let delta = positions[link.target] - positions[link.source];
            let length = delta.x.hypot(delta.y).max(1.);
            let force = delta * ((length - distance) / length * 0.06);
            forces[link.source] = forces[link.source] + force;
            forces[link.target] = forces[link.target] - force;
        }
        // Sample local cells with one centroid per cell to bound dense-graph work.
        let mut cells = BTreeMap::<_, (Point, u32)>::new();
        for &pos in &positions {
            cancel.check()?;
            let entry = cells
                .entry(cell(pos, Point::new(distance, distance)))
                .or_default();
            entry.0 = entry.0 + pos;
            entry.1 += 1;
        }
        for (id, &pos) in positions.iter().enumerate() {
            cancel.check()?;
            let (x, y) = cell(pos, Point::new(distance, distance));
            for (cx, cy) in neighbors(x, y) {
                if let Some(&(mut sum, mut count)) = cells.get(&(cx, cy)) {
                    if (cx, cy) == (x, y) {
                        sum = sum - pos;
                        count -= 1;
                    }
                    if count > 0 {
                        let mut delta = pos - sum * (1. / count as f32);
                        if delta == Point::default() {
                            delta = Point::new(next(&mut seed), next(&mut seed));
                        }
                        let length = delta.x.hypot(delta.y).max(1.);
                        forces[id] = forces[id] + delta * (distance / length).min(4.) * 0.12;
                    }
                }
            }
        }
        let cap = distance * 0.25 * (1. - step as f32 / iterations as f32);
        for ((pos, force), unit) in positions.iter_mut().zip(&forces).zip(units) {
            if !unit.fixed {
                *pos = *pos + Point::new(force.x.clamp(-cap, cap), force.y.clamp(-cap, cap));
            }
        }
    }
    Ok(positions)
}

/// Translate each affected connected component toward its previous position.
/// A median offset minimizes the sum of axis-wise movement for a fixed layout.
pub(super) fn anchor_affected(
    units: &[Unit],
    links: &[Link],
    positions: &mut [Point],
    cancel: &LayoutCancellation,
) -> Result<(), LayoutError> {
    let mut adjacent = vec![Vec::new(); units.len()];
    for link in links {
        cancel.check()?;
        if !units[link.source].fixed && !units[link.target].fixed {
            adjacent[link.source].push(link.target);
            adjacent[link.target].push(link.source);
        }
    }
    let mut visited: Vec<_> = units.iter().map(|unit| unit.fixed).collect();
    let mut stack = Vec::new();
    let mut members = Vec::new();
    let mut x = Vec::new();
    let mut y = Vec::new();
    for root in 0..units.len() {
        cancel.check()?;
        if visited[root] {
            continue;
        }
        visited[root] = true;
        stack.push(root);
        members.clear();
        x.clear();
        y.clear();
        while let Some(id) = stack.pop() {
            cancel.check()?;
            members.push(id);
            let delta = units[id].bounds.origin - positions[id];
            x.push(delta.x);
            y.push(delta.y);
            for &other in &adjacent[id] {
                if !visited[other] {
                    visited[other] = true;
                    stack.push(other);
                }
            }
        }
        let middle = members.len() / 2;
        let (_, &mut dx, _) = x.select_nth_unstable_by(middle, f32::total_cmp);
        let (_, &mut dy, _) = y.select_nth_unstable_by(middle, f32::total_cmp);
        for &id in &members {
            positions[id] = positions[id] + Point::new(dx, dy);
        }
    }
    Ok(())
}

type Constraint = (usize, usize, usize, bool, Point, Point, Point);

pub(super) fn resolve(
    units: &[Unit],
    positions: &mut [Point],
    constraints: &[Constraint],
    spacing: Point,
    cancel: &LayoutCancellation,
) -> Result<(), LayoutError> {
    let mut reserved: Vec<_> = units.iter().map(|unit| unit.fixed).collect();
    for &(a, b, _, _, _, _, _) in constraints {
        reserved[a] = true;
        reserved[b] = true;
    }
    let preferred = positions.to_vec();
    let mut constraints = constraints.to_vec();
    solve(units, positions, &constraints, spacing, cancel)?;
    // Reserve constrained groups first. If an alignment puts two rectangles
    // together, repair on the other axis whenever the hard constraints allow it.
    let cell_size = cell_extent(units, spacing);
    let mut cells = BTreeMap::<_, Vec<(usize, Rect)>>::new();
    let mut repairs = 0;
    loop {
        cells.clear();
        let mut collision = None;
        for (id, &pos) in positions.iter().enumerate().filter(|(id, _)| reserved[*id]) {
            cancel.check()?;
            let rect = Rect {
                origin: pos,
                size: units[id].bounds.size + spacing,
            };
            if !rect.valid() {
                return Err(LayoutError::Infeasible);
            }
            let key = cell(pos, cell_size);
            if let Some(&(other, _)) = neighbors(key.0, key.1)
                .filter_map(|key| cells.get(&key))
                .flatten()
                .find(|(_, other)| overlaps(rect, *other))
            {
                collision = Some((other, id));
                break;
            }
            cells.entry(key).or_default().push((id, rect));
        }
        let Some((a, b)) = collision else {
            break;
        };
        if units[a].fixed && units[b].fixed || repairs >= units.len() {
            return Err(LayoutError::Infeasible);
        }
        repairs += 1;
        let mut accepted = false;
        for (axis, first, second) in [(0, a, b), (1, a, b), (0, b, a), (1, b, a)] {
            constraints.push((
                first,
                second,
                axis,
                false,
                Point::default(),
                Point::default(),
                units[first].bounds.size,
            ));
            positions.copy_from_slice(&preferred);
            match solve(units, positions, &constraints, spacing, cancel) {
                Ok(()) => {
                    accepted = true;
                    break;
                }
                Err(LayoutError::Infeasible) => {
                    constraints.pop();
                }
                Err(error) => return Err(error),
            }
        }
        if !accepted {
            return Err(LayoutError::Infeasible);
        }
    }

    for (id, pos) in positions
        .iter_mut()
        .enumerate()
        .filter(|(id, _)| !reserved[*id])
    {
        loop {
            cancel.check()?;
            let rect = Rect {
                origin: *pos,
                size: units[id].bounds.size + spacing,
            };
            if !rect.valid() {
                return Err(LayoutError::Infeasible);
            }
            let key = cell(*pos, cell_size);
            let next = neighbors(key.0, key.1)
                .filter_map(|key| cells.get(&key))
                .flatten()
                .filter(|(_, other)| overlaps(rect, *other))
                .map(|(_, other)| other.max().x)
                .max_by(f32::total_cmp);
            if let Some(x) = next {
                pos.x = x;
            } else {
                cells.entry(key).or_default().push((id, rect));
                break;
            }
        }
    }
    Ok(())
}

fn solve(
    units: &[Unit],
    positions: &mut [Point],
    constraints: &[Constraint],
    spacing: Point,
    cancel: &LayoutCancellation,
) -> Result<(), LayoutError> {
    for (pos, unit) in positions.iter_mut().zip(units) {
        if unit.fixed {
            *pos = unit.bounds.origin;
        }
    }
    if constraints.is_empty() {
        return Ok(());
    }
    for axis in 0..2 {
        let component = |p: Point| if axis == 0 { p.x } else { p.y };
        let mut values: Vec<_> = positions
            .iter()
            .map(|&pos| f64::from(component(pos)))
            .collect();
        let mut ordered = vec![Vec::new(); units.len()];
        let mut reversed = vec![Vec::new(); units.len()];
        let mut add = |a: usize, b: usize, gap| {
            ordered[a].push((b, gap));
            reversed[b].push((a, gap));
        };
        for &(a, b, dim, aligned, offset_a, offset_b, size) in constraints {
            cancel.check()?;
            if dim != axis {
                continue;
            }
            let offset = f64::from(component(offset_a)) - f64::from(component(offset_b));
            if aligned {
                add(a, b, offset);
                add(b, a, -offset);
            } else {
                add(a, b, offset + f64::from(component(size + spacing)));
            }
        }
        // A fixed coordinate bounds every predecessor from above. Propagate
        // the negated bounds backwards, then propagate the lower bounds forwards.
        let mut negated_upper: Vec<_> = values
            .iter()
            .zip(units)
            .map(
                |(&value, u)| {
                    if u.fixed { -value } else { f64::NEG_INFINITY }
                },
            )
            .collect();
        propagate(&mut negated_upper, &reversed, units, cancel)?;
        for (value, bound) in values.iter_mut().zip(negated_upper) {
            *value = value.min(-bound);
        }
        propagate(&mut values, &ordered, units, cancel)?;
        for (pos, value) in positions.iter_mut().zip(values) {
            if axis == 0 {
                pos.x = value as f32;
            } else {
                pos.y = value as f32;
            }
        }
    }

    Ok(())
}

fn propagate(
    positions: &mut [f64],
    edges: &[Vec<(usize, f64)>],
    units: &[Unit],
    cancel: &LayoutCancellation,
) -> Result<(), LayoutError> {
    let mut queue = VecDeque::new();
    let mut queued = vec![false; units.len()];
    let mut distance = vec![0; units.len()];
    for (id, (&value, targets)) in positions.iter().zip(edges).enumerate() {
        if value.is_finite() && !targets.is_empty() {
            queue.push_back(id);
            queued[id] = true;
        }
    }
    while let Some(a) = queue.pop_front() {
        queued[a] = false;
        for &(b, gap) in &edges[a] {
            cancel.check()?;
            let minimum = positions[a] + gap;
            if minimum <= positions[b] {
                continue;
            }
            distance[b] = distance[a] + 1;
            if units[b].fixed || distance[b] >= units.len() {
                return Err(LayoutError::Infeasible);
            }
            positions[b] = minimum;
            if !queued[b] && !edges[b].is_empty() {
                queued[b] = true;
                queue.push_back(b);
            }
        }
    }
    Ok(())
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.origin.x < b.max().x
        && a.max().x > b.origin.x
        && a.origin.y < b.max().y
        && a.max().y > b.origin.y
}

fn cell_extent(units: &[Unit], spacing: Point) -> Point {
    units.iter().fold(Point::new(1., 1.), |size, unit| {
        let extent = unit.bounds.size + spacing;
        Point::new(size.x.max(extent.x), size.y.max(extent.y))
    })
}

fn cell(pos: Point, size: Point) -> (i64, i64) {
    (
        (pos.x / size.x).floor() as i64,
        (pos.y / size.y).floor() as i64,
    )
}

fn neighbors(x: i64, y: i64) -> impl Iterator<Item = (i64, i64)> {
    (-1..=1)
        .flat_map(move |dx| (-1..=1).map(move |dy| (x.saturating_add(dx), y.saturating_add(dy))))
}
