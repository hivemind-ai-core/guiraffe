use super::{AutoLayout, LayoutCancellation, LayoutError, LayoutOptions, Link, Unit};
use crate::Point;
use std::collections::BTreeSet;

type Adjacency = Vec<Vec<(usize, f32, f32)>>;

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
    let back_edges = find_back_edges(&outgoing, cancel)?;
    let mut pending: Vec<_> = incoming
        .iter()
        .enumerate()
        .map(|(id, incoming)| {
            incoming
                .iter()
                .filter(|&&(source, _, _)| !back_edges.contains(&(source, id)))
                .count()
        })
        .collect();
    let mut ready: BTreeSet<_> = (0..units.len()).filter(|&id| pending[id] == 0).collect();
    let mut ranks = vec![0; units.len()];
    let mut layers = Vec::<Vec<usize>>::new();
    while let Some(id) = ready.pop_first() {
        cancel.check()?;
        let rank = ranks[id];
        if layers.len() <= rank {
            layers.resize_with(rank + 1, Vec::new);
        }
        layers[rank].push(id);
        for &(next, _, _) in &outgoing[id] {
            if !back_edges.contains(&(id, next)) {
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
    let mut best = layers.clone();
    let mut best_crossings = if sweeps > 0 {
        crossing_count(links, &ranks, &order, cancel)?
    } else {
        0
    };
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
                let improvement = |adjacent: &Adjacency| {
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
        let crossings = crossing_count(links, &ranks, &order, cancel)?;
        if crossings < best_crossings {
            best_crossings = crossings;
            best.clone_from(&layers);
        }
    }
    layers = best;
    for layer in &layers {
        record_order(layer, &mut order);
    }

    let horizontal = options.direction.horizontal();
    let axes = |p: Point| if horizontal { (p.x, p.y) } else { (p.y, p.x) };
    let (rank_gap, node_gap) = axes(options.spacing);
    let tree_cross = if matches!(engine, AutoLayout::Tree) {
        Some(tree_positions(
            units,
            &layers,
            (&ranks, &order),
            &incoming,
            horizontal,
            node_gap,
            cancel,
        )?)
    } else {
        None
    };
    let mut positions = vec![Point::default(); units.len()];
    let mut depth = 0.;
    let mut radius = 0.;
    for (rank, layer) in layers.iter().enumerate() {
        cancel.check()?;
        let mut cross = 0.;
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
            let cross_position = tree_cross.as_ref().map_or(cross, |tree| tree[id]);
            positions[id] = if radial {
                let angle = std::f32::consts::TAU * slot as f32 / layer.len() as f32;
                Point::new(angle.cos(), angle.sin()) * radius - size * 0.5
            } else if horizontal {
                Point::new(primary, cross_position)
            } else {
                Point::new(cross_position, primary)
            };
            thickness = thickness.max(length);
            cross += width + node_gap;
        }
        depth += thickness + rank_gap;
        radius += diameter / 2.;
    }
    if options.direction.reversed() && !matches!(engine, AutoLayout::Radial) {
        let shift = (depth - rank_gap).max(0.);
        let offset = if horizontal {
            Point::new(shift, 0.)
        } else {
            Point::new(0., shift)
        };
        for position in &mut positions {
            *position = *position + offset;
        }
    }
    Ok(positions)
}

fn find_back_edges(
    outgoing: &Adjacency,
    cancel: &LayoutCancellation,
) -> Result<BTreeSet<(usize, usize)>, LayoutError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        New,
        Visiting,
        Complete,
    }
    let mut visits = vec![Visit::New; outgoing.len()];
    let mut stack = Vec::new();
    let mut back_edges = BTreeSet::new();
    for start in 0..outgoing.len() {
        cancel.check()?;
        if visits[start] != Visit::New {
            continue;
        }
        stack.push((start, 0));
        visits[start] = Visit::Visiting;
        while let Some((id, next)) = stack.last_mut() {
            cancel.check()?;
            if let Some(&(target, _, _)) = outgoing[*id].get(*next) {
                *next += 1;
                match visits[target] {
                    Visit::New => {
                        visits[target] = Visit::Visiting;
                        stack.push((target, 0));
                    }
                    Visit::Visiting => {
                        back_edges.insert((*id, target));
                    }
                    Visit::Complete => {}
                }
            } else {
                visits[*id] = Visit::Complete;
                stack.pop();
            }
        }
    }
    Ok(back_edges)
}

fn record_order(layer: &[usize], order: &mut [usize]) {
    for (slot, &id) in layer.iter().enumerate() {
        order[id] = slot;
    }
}

/// Select a deterministic spanning forest and center every parent over its
/// children's combined span. Cross-links remain routed in the result.
fn tree_positions(
    units: &[Unit],
    layers: &[Vec<usize>],
    (ranks, order): (&[usize], &[usize]),
    incoming: &Adjacency,
    horizontal: bool,
    gap: f32,
    cancel: &LayoutCancellation,
) -> Result<Vec<f32>, LayoutError> {
    let mut children = vec![Vec::new(); units.len()];
    let mut roots = Vec::new();
    let width = |id: usize| {
        let size = units[id].bounds.size;
        if horizontal { size.y } else { size.x }
    };
    for layer in layers {
        for &id in layer {
            cancel.check()?;
            let parent = incoming[id]
                .iter()
                .map(|&(parent, _, _)| parent)
                .filter(|&parent| ranks[parent] < ranks[id])
                .max_by_key(|&parent| {
                    (
                        ranks[parent],
                        std::cmp::Reverse(order[parent]),
                        std::cmp::Reverse(parent),
                    )
                });
            if let Some(parent) = parent {
                children[parent].push(id);
            } else {
                roots.push(id);
            }
        }
    }
    let mut spans = vec![0.; units.len()];
    for layer in layers.iter().rev() {
        for &id in layer {
            cancel.check()?;
            spans[id] = width(id).max(child_span(&children[id], &spans, gap));
        }
    }
    let mut positions = vec![0.; units.len()];
    let mut stack = Vec::new();
    let mut offset = 0.;
    for id in roots {
        stack.push((id, offset));
        offset += spans[id] + gap;
    }
    while let Some((id, left)) = stack.pop() {
        cancel.check()?;
        positions[id] = left + (spans[id] - width(id)) * 0.5;
        let mut child_left = left + (spans[id] - child_span(&children[id], &spans, gap)) * 0.5;
        for &child in &children[id] {
            stack.push((child, child_left));
            child_left += spans[child] + gap;
        }
    }
    Ok(positions)
}

fn child_span(children: &[usize], spans: &[f32], gap: f32) -> f32 {
    children.iter().map(|&id| spans[id]).sum::<f32>()
        + children.len().saturating_sub(1) as f32 * gap
}

/// Count inversions between distinct ports of links sharing the same rank pair.
fn crossing_count(
    links: &[Link],
    ranks: &[usize],
    order: &[usize],
    cancel: &LayoutCancellation,
) -> Result<u64, LayoutError> {
    let mut edges = Vec::with_capacity(links.len());
    for link in links {
        cancel.check()?;
        let (a, b) = (ranks[link.source], ranks[link.target]);
        let source = (order[link.source], link.source_port);
        let target = (order[link.target], link.target_port);
        if a < b {
            edges.push(((a, b), source, target));
        } else if a > b {
            edges.push(((b, a), target, source));
        }
    }
    let port_cmp =
        |a: &(usize, f32), b: &(usize, f32)| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1));
    edges.sort_unstable_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| port_cmp(&a.1, &b.1))
            .then_with(|| port_cmp(&a.2, &b.2))
    });
    let mut count = 0;
    let mut ports = Vec::new();
    let mut tree = Vec::<u64>::new();
    for layer in edges.chunk_by(|a, b| a.0 == b.0) {
        cancel.check()?;
        ports.clear();
        ports.extend(layer.iter().map(|edge| edge.2));
        ports.sort_unstable_by(port_cmp);
        ports.dedup();
        tree.clear();
        tree.resize(ports.len() + 1, 0);
        for (seen, &(_, _, port)) in layer.iter().enumerate() {
            cancel.check()?;
            let mut index = ports.partition_point(|p| port_cmp(p, &port).is_lt()) + 1;
            let mut prefix = index;
            let mut non_crossing = 0;
            while prefix != 0 {
                non_crossing += tree[prefix];
                prefix &= prefix - 1;
            }
            count += seen as u64 - non_crossing;
            while index < tree.len() {
                tree[index] += 1;
                index += 1 << index.trailing_zeros();
            }
        }
    }
    Ok(count)
}
