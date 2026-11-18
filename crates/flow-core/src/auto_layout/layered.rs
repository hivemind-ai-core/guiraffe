use super::{AutoLayout, LayoutCancellation, LayoutError, LayoutOptions, Link, Unit};
use crate::Point;
use std::collections::BTreeSet;

pub(super) fn positions(
    units: &[Unit],
    links: &[Link],
    options: &LayoutOptions,
    engine: AutoLayout,
    cancel: &LayoutCancellation,
) -> Result<Vec<Point>, LayoutError> {
    let mut incoming = vec![Vec::new(); units.len()];
    let mut outgoing = vec![Vec::new(); units.len()];
    for link in links {
        incoming[link.target].push((link.source, link.source_port, link.target_port));
        outgoing[link.source].push((link.target, link.target_port, link.source_port));
    }
    let mut pending: Vec<_> = incoming.iter().map(Vec::len).collect();
    let mut unplaced: BTreeSet<_> = (0..units.len()).collect();
    let mut ready: BTreeSet<_> = unplaced
        .iter()
        .copied()
        .filter(|&i| pending[i] == 0)
        .collect();
    let mut ranks = vec![0; units.len()];
    let mut layers = Vec::<Vec<usize>>::new();
    while let Some(id) = ready.pop_first().or_else(|| unplaced.first().copied()) {
        cancel.check()?;
        unplaced.remove(&id);
        let rank = ranks[id];
        if layers.len() <= rank {
            layers.resize_with(rank + 1, Vec::new);
        }
        layers[rank].push(id);
        for &(next, _, _) in &outgoing[id] {
            if unplaced.contains(&next) {
                ranks[next] = ranks[next].max(rank + 1);
                pending[next] -= 1;
                if pending[next] == 0 {
                    ready.insert(next);
                }
            }
        }
    }

    let mut order = vec![0; units.len()];
    for layer in &mut layers {
        layer.sort_by_key(|&id| (units[id].order, id));
        record_order(layer, &mut order);
    }
    let sweeps = (options.crossing_weight * 4.).ceil().min(16.) as usize;
    let mut score = vec![0.; units.len()];
    for sweep in 0..sweeps {
        let forward = sweep % 2 == 0;
        let adjacent = if forward { &incoming } else { &outgoing };
        for step in 0..layers.len() {
            cancel.check()?;
            let rank = if forward {
                step
            } else {
                layers.len() - step - 1
            };
            let layer = &mut layers[rank];
            for &id in layer.iter() {
                let mut sum = 0.;
                let mut count = 0;
                for &(other, port, own_port) in &adjacent[id] {
                    if ranks[other] != rank {
                        sum += order[other] as f64
                            + (f64::from(port) - f64::from(own_port)).atan() * 0.25;
                        count += 1;
                    }
                }
                score[id] = if count == 0 {
                    order[id] as f64
                } else {
                    sum / f64::from(count)
                };
            }
            layer.sort_by(|&a, &b| {
                units[a]
                    .order
                    .cmp(&units[b].order)
                    .then_with(|| score[a].total_cmp(&score[b]))
            });
            for pos in 1..layer.len() {
                let (a, b) = (layer[pos - 1], layer[pos]);
                if units[a].order != units[b].order {
                    continue;
                }
                let improvement = |adjacent: &[Vec<(usize, f32, f32)>]| {
                    let mut change = 0;
                    // Bound the local refinement for high-degree hubs.
                    if adjacent[a].len().saturating_mul(adjacent[b].len()) <= 4096 {
                        for &(x, xp, _) in &adjacent[a] {
                            for &(y, yp, _) in &adjacent[b] {
                                if ranks[x] == ranks[y] && ranks[x] != rank {
                                    let cmp =
                                        order[x].cmp(&order[y]).then_with(|| xp.total_cmp(&yp));
                                    change += cmp as i32;
                                }
                            }
                        }
                    }
                    change
                };
                if improvement(&incoming) + improvement(&outgoing) > 0 {
                    layer.swap(pos - 1, pos);
                }
            }
            record_order(layer, &mut order);
        }
    }

    let horizontal = options.direction.horizontal();
    let axes = |p: Point| if horizontal { (p.x, p.y) } else { (p.y, p.x) };
    let (rank_gap, node_gap) = axes(options.spacing);
    let mut positions = vec![Point::default(); units.len()];
    let mut depth = 0.;
    let mut radius = 0.;
    for (rank, layer) in layers.iter().enumerate() {
        cancel.check()?;
        let span = layer
            .iter()
            .map(|&id| axes(units[id].bounds.size).1 + node_gap)
            .sum::<f32>()
            - node_gap;
        let mut cross = if matches!(engine, AutoLayout::Tree) {
            -span / 2.
        } else {
            0.
        };
        let mut thickness: f32 = 0.;
        let diameter = layer
            .iter()
            .map(|&id| {
                let size = units[id].bounds.size + options.spacing;
                size.x.hypot(size.y)
            })
            .fold(0., f32::max);
        let radial = matches!(engine, AutoLayout::Radial);
        if radial && !(rank == 0 && layer.len() == 1) {
            let chord = (std::f32::consts::PI / layer.len().max(2) as f32).sin();
            radius = (radius + diameter / 2.).max(diameter / (2. * chord));
        }
        for (slot, &id) in layer.iter().enumerate() {
            let size = units[id].bounds.size;
            let (length, width) = axes(size);
            let primary = if options.direction.reversed() {
                -depth - length
            } else {
                depth
            };
            positions[id] = if radial {
                let angle = std::f32::consts::TAU * slot as f32 / layer.len() as f32;
                Point::new(angle.cos(), angle.sin()) * radius - size * 0.5
            } else if horizontal {
                Point::new(primary, cross)
            } else {
                Point::new(cross, primary)
            };
            thickness = thickness.max(length);
            cross += width + node_gap;
        }
        depth += thickness + rank_gap;
        radius += diameter / 2.;
    }
    Ok(positions)
}

fn record_order(layer: &[usize], order: &mut [usize]) {
    for (slot, &id) in layer.iter().enumerate() {
        order[id] = slot;
    }
}
