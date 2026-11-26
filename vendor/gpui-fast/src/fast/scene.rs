// Modified by Longbridge for gpui-fast.
//! Putting a finished scene in drawing order without moving its primitives
//! more than once or allocating, and scene helpers for tests: a finished scene
//! described as text, to compare two frames by, and forgetting the orderings
//! the bounds tree replays.

use crate::{
    DrawOrder, MonochromeSprite, PaintSurface, Path, PolychromeSprite, Quad, ScaledPixels, Scene,
    Shadow, SubpixelSprite, Underline,
};
use std::mem;

/// Room to sort a scene's primitives in, kept from one frame to the next so
/// that a frame does not allocate megabytes to put what it drew in order.
#[derive(Default)]
pub(crate) struct SortScratch {
    permutation: Permutation,
    shadows: Vec<Shadow>,
    quads: Vec<Quad>,
    paths: Vec<Path<ScaledPixels>>,
    underlines: Vec<Underline>,
    monochrome_sprites: Vec<MonochromeSprite>,
    subpixel_sprites: Vec<SubpixelSprite>,
    polychrome_sprites: Vec<PolychromeSprite>,
    surfaces: Vec<PaintSurface>,
}

/// The order to gather a kind of primitive in, and what working it out takes.
#[derive(Default)]
struct Permutation {
    /// The index of the primitive to put at each place.
    order: Vec<u32>,
    /// For each draw order, how many primitives have it, and then where the
    /// next of them goes.
    counts: Vec<u32>,
    /// Draw orders packed with indices, when they are too spread out to count.
    keys: Vec<u64>,
    /// Sprites' draw orders, textures and tiles packed with indices.
    wide_keys: Vec<u128>,
}

/// How much wider than the number of primitives their draw orders may spread
/// before [`sort_by_draw_order`] sorts them rather than counting them, so that
/// the counts are never much longer than the primitives.
const MAX_ORDERS_PER_PRIMITIVE: usize = 4;

/// Puts `items` in draw order, keeping the order they came in among those of
/// one draw order, as a stable sort does.
///
/// A primitive's draw order is one past the greatest among the primitives
/// before it that it overlaps, or its layer's, so a scene of thousands of
/// primitives has a few dozen of them. They are counted rather than compared:
/// one pass counts how many primitives have each order, the next puts each
/// where its order's run is up to. Orders spread wider than
/// [`MAX_ORDERS_PER_PRIMITIVE`] allows are sorted as packed keys instead. A
/// scene with no primitive out of order is left as it is.
fn sort_by_draw_order<T: Clone>(
    items: &mut Vec<T>,
    permutation: &mut Permutation,
    gathered: &mut Vec<T>,
    slots: &mut Vec<u32>,
    draw_order: impl Fn(&T) -> DrawOrder,
) {
    if items.len() < 2 {
        identity_slots(slots, items.len());
        return;
    }
    let mut max = 0;
    let mut sorted = true;
    let mut previous = 0;
    for item in items.iter() {
        let order = draw_order(item);
        sorted &= order >= previous;
        previous = order;
        max = max.max(order);
    }
    if sorted {
        identity_slots(slots, items.len());
        return;
    }
    let Permutation {
        order,
        counts,
        keys,
        ..
    } = permutation;
    if max as usize > items.len() * MAX_ORDERS_PER_PRIMITIVE {
        keys.clear();
        keys.extend(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| (u64::from(draw_order(item)) << 32) | index as u64),
        );
        keys.sort_unstable();
        order.clear();
        order.extend(keys.iter().map(|&key| key as u32));
    } else {
        counts.clear();
        counts.resize(max as usize + 1, 0);
        for item in items.iter() {
            counts[draw_order(item) as usize] += 1;
        }
        let mut start = 0;
        for count in counts.iter_mut() {
            let primitives = *count;
            *count = start;
            start += primitives;
        }
        order.clear();
        order.resize(items.len(), 0);
        for (index, item) in items.iter().enumerate() {
            let next = &mut counts[draw_order(item) as usize];
            order[*next as usize] = index as u32;
            *next += 1;
        }
    }
    inverse_slots(slots, order);
    gather(items, order, gathered);
}

/// Puts sprites in draw order, those of one draw order grouped by the atlas
/// texture they come from and then by tile, keeping the order they came in
/// among sprites of one tile, as a stable sort does.
///
/// The draw order, texture, tile and index of each sprite are packed into one
/// integer, so the sort compares integers lying next to each other rather
/// than reaching into a sprite for its key on every comparison. Sprites
/// already in that order are left as they are.
fn sort_sprites<T: Clone>(
    items: &mut Vec<T>,
    permutation: &mut Permutation,
    gathered: &mut Vec<T>,
    slots: &mut Vec<u32>,
    key: impl Fn(&T) -> (DrawOrder, u32, u32),
) {
    if items.len() < 2 {
        identity_slots(slots, items.len());
        return;
    }
    let Permutation {
        order, wide_keys, ..
    } = permutation;
    wide_keys.clear();
    wide_keys.extend(items.iter().enumerate().map(|(index, item)| {
        let (draw_order, texture, tile) = key(item);
        (u128::from(draw_order) << 96)
            | (u128::from(texture) << 64)
            | (u128::from(tile) << 32)
            | index as u128
    }));
    if wide_keys.is_sorted() {
        identity_slots(slots, items.len());
        return;
    }
    wide_keys.sort_unstable();
    order.clear();
    order.extend(wide_keys.iter().map(|&key| key as u32));
    inverse_slots(slots, order);
    gather(items, order, gathered);
}

/// Where each of `len` primitives left in the order they came in lies.
fn identity_slots(slots: &mut Vec<u32>, len: usize) {
    slots.clear();
    slots.extend(0..len as u32);
}

/// Where each primitive lies once the one at `order[i]` is put at place `i`.
fn inverse_slots(slots: &mut Vec<u32>, order: &[u32]) {
    slots.clear();
    slots.resize(order.len(), 0);
    for (place, &index) in order.iter().enumerate() {
        slots[index as usize] = place as u32;
    }
}

/// Where each primitive of a finished scene lies in its kind's list, for a
/// patched frame to overwrite one where it lies (see [`crate::fast::patch`]).
#[derive(Default)]
pub(crate) struct SceneSlots {
    /// For each kind of primitive, in the order of [`Scene::insert_primitive`]'s
    /// kinds, where the primitive inserted `i`th went.
    kinds: [Vec<u32>; 8],
    /// For each paint operation, where its primitive lies in its kind's list,
    /// worked out when a patch first needs it. `u32::MAX` for operations that
    /// are not primitives.
    operations: Vec<u32>,
    /// For each paint operation, which insertion into the scene's bounds
    /// tree it made: a layer, or a primitive outside any layer, which takes
    /// its draw order from the tree. `u32::MAX` for the others: a primitive
    /// in a layer takes the layer's.
    insertions: Vec<u32>,
    /// Whether `kinds` stands for the scene: it was put in order by
    /// [`sort_in_drawing_order`], without rounded clip groups that rewrite it.
    valid: bool,
}

impl SceneSlots {
    pub(crate) fn clear(&mut self) {
        self.valid = false;
        self.operations.clear();
        self.insertions.clear();
    }

    /// Where the primitive of each paint operation lies in its kind's list,
    /// and which insertion into the bounds tree it made (see
    /// [`SceneSlots::insertions`]), or `None` if the scene was not put in
    /// order in a way that tells.
    pub(crate) fn operations(scene: &mut Scene) -> Option<(&[u32], &[u32])> {
        let slots = &mut scene.fast_slots;
        if !slots.valid {
            return None;
        }
        if slots.operations.len() != scene.paint_operations.len() {
            let mut counts = [0usize; 8];
            let (mut inserted, mut depth) = (0u32, 0usize);
            slots.operations.clear();
            slots.insertions.clear();
            for operation in &scene.paint_operations {
                let (slot, insertion) = match operation {
                    crate::PaintOperation::Primitive(primitive) => {
                        let kind = primitive_kind(primitive);
                        let slot = slots.kinds[kind].get(counts[kind]).copied();
                        counts[kind] += 1;
                        let insertion = if depth == 0 {
                            inserted += 1;
                            inserted - 1
                        } else {
                            u32::MAX
                        };
                        (slot?, insertion)
                    }
                    crate::PaintOperation::StartLayer(_) => {
                        depth += 1;
                        inserted += 1;
                        (u32::MAX, inserted - 1)
                    }
                    crate::PaintOperation::EndLayer => {
                        depth = depth.saturating_sub(1);
                        (u32::MAX, u32::MAX)
                    }
                    crate::PaintOperation::FastRoundedClip(_) => (u32::MAX, u32::MAX),
                };
                slots.operations.push(slot);
                slots.insertions.push(insertion);
            }
        }
        Some((&slots.operations, &slots.insertions))
    }
}

/// The index of `primitive`'s kind among [`SceneSlots::kinds`].
pub(crate) fn primitive_kind(primitive: &crate::Primitive) -> usize {
    match primitive {
        crate::Primitive::Shadow(_) => 0,
        crate::Primitive::Quad(_) => 1,
        crate::Primitive::Path(_) => 2,
        crate::Primitive::Underline(_) => 3,
        crate::Primitive::MonochromeSprite(_) => 4,
        crate::Primitive::SubpixelSprite(_) => 5,
        crate::Primitive::PolychromeSprite(_) => 6,
        crate::Primitive::Surface(_) => 7,
    }
}

/// Puts the item at `order[i]` at place `i`.
///
/// Sorting indices and then gathering once moves each item a single time,
/// where sorting the items themselves moves them as often as the sort needs
/// to compare them — and these items are 112 to 168 bytes each.
fn gather<T: Clone>(items: &mut Vec<T>, order: &[u32], gathered: &mut Vec<T>) {
    gathered.clear();
    gathered.extend(order.iter().map(|&index| items[index as usize].clone()));
    mem::swap(items, gathered);
}

impl Scene {
    /// Forgets the orderings recorded for replaying, so the next frame orders
    /// every primitive from scratch.
    #[cfg(test)]
    pub(crate) fn forget_orderings(&mut self) {
        self.primitive_bounds.forget();
    }

    /// Everything this finished scene draws, in drawing order, as text two
    /// scenes can be compared by: each primitive with its bounds, clip, colours
    /// and ordering, and each layer's bounds. Atlas tiles are left out, since
    /// two windows need not place the same glyph in the same tile.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for operation in &self.paint_operations {
            match operation {
                crate::PaintOperation::StartLayer(bounds) => {
                    lines.push(format!("layer {bounds:?}"))
                }
                crate::PaintOperation::EndLayer => lines.push("end layer".into()),
                crate::PaintOperation::FastRoundedClip(clip) => {
                    lines.push(format!("rounded clip {clip:?}"))
                }
                crate::PaintOperation::Primitive(..) => {}
            }
        }
        lines.extend(self.shadows.iter().map(|shadow| format!("{shadow:?}")));
        lines.extend(self.quads.iter().map(|quad| format!("{quad:?}")));
        lines.extend(
            self.underlines
                .iter()
                .map(|underline| format!("{underline:?}")),
        );
        lines.extend(self.monochrome_sprites.iter().map(|sprite| {
            format!(
                "monochrome sprite {} {:?} {:?} {:?}",
                sprite.order, sprite.bounds, sprite.content_mask, sprite.color
            )
        }));
        lines.extend(self.subpixel_sprites.iter().map(|sprite| {
            format!(
                "subpixel sprite {} {:?} {:?} {:?}",
                sprite.order, sprite.bounds, sprite.content_mask, sprite.color
            )
        }));
        lines.extend(self.polychrome_sprites.iter().map(|sprite| {
            format!(
                "polychrome sprite {} {:?} {:?}",
                sprite.order, sprite.bounds, sprite.content_mask
            )
        }));
        lines.extend(
            self.paths
                .iter()
                .map(|path| format!("path {} {:?}", path.order, path.bounds)),
        );
        lines
    }
}

/// What [`Scene::finish`] does: puts every primitive in drawing order.
/// Sprites of one order are grouped by the atlas texture they come from:
/// a batch draws from one texture, and tile ids, which each texture
/// numbers from zero, would interleave them.
#[inline]
pub(crate) fn sort_in_drawing_order(scene: &mut Scene) {
    let clip_scopes = !scene.fast_rounded_clip.scopes.is_empty();
    crate::fast::rounded_clip_scene::compile(scene);
    let scratch = &mut scene.sort_scratch;
    let slots = &mut scene.fast_slots;
    slots.operations.clear();
    slots.insertions.clear();
    // Rounded clip groups rewrite the scene into nested ones; its primitives
    // are then not where its paint operations say.
    slots.valid = !clip_scopes;
    let kinds = &mut slots.kinds;
    macro_rules! sort {
        ($field:ident, $kind:expr, $order:expr) => {
            sort_by_draw_order(
                &mut scene.$field,
                &mut scratch.permutation,
                &mut scratch.$field,
                &mut kinds[$kind],
                $order,
            )
        };
    }
    macro_rules! sort_sprites {
        ($field:ident, $kind:expr) => {
            sort_sprites(
                &mut scene.$field,
                &mut scratch.permutation,
                &mut scratch.$field,
                &mut kinds[$kind],
                |sprite| {
                    (
                        sprite.order,
                        sprite.tile.texture_id.index,
                        sprite.tile.tile_id.0,
                    )
                },
            )
        };
    }
    sort!(shadows, 0, |shadow: &Shadow| shadow.order);
    sort!(quads, 1, |quad: &Quad| quad.order);
    sort!(paths, 2, |path: &Path<ScaledPixels>| path.order);
    sort!(underlines, 3, |underline: &Underline| underline.order);
    sort_sprites!(monochrome_sprites, 4);
    sort_sprites!(subpixel_sprites, 5);
    sort_sprites!(polychrome_sprites, 6);
    sort!(surfaces, 7, |surface: &PaintSurface| surface.order);
}

#[cfg(test)]
mod tests {
    use super::{MAX_ORDERS_PER_PRIMITIVE, Permutation, sort_by_draw_order, sort_sprites};
    use crate::DrawOrder;
    use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

    /// A primitive as the sorts see it, and where it was before them.
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Item {
        order: DrawOrder,
        texture: u32,
        tile: u32,
        painted: usize,
    }

    /// Random primitives with draw orders below `orders`, from few textures
    /// and tiles so that many share them.
    fn items(rng: &mut StdRng, len: usize, orders: DrawOrder) -> Vec<Item> {
        (0..len)
            .map(|painted| Item {
                order: rng.random_range(0..orders),
                texture: rng.random_range(0..3),
                tile: rng.random_range(0..8),
                painted,
            })
            .collect()
    }

    /// Lengths and order spreads covering no items, one, orders already in
    /// order, orders counted, and orders spread too wide to count.
    fn cases(rng: &mut StdRng) -> Vec<Vec<Item>> {
        let mut cases = vec![Vec::new(), items(rng, 1, 5)];
        for _ in 0..200 {
            let len = rng.random_range(2..300);
            let spread = match rng.random_range(0..4) {
                0 => 1,
                1 => rng.random_range(2..30),
                2 => (len * MAX_ORDERS_PER_PRIMITIVE + 1) as DrawOrder,
                _ => rng.random_range(len * MAX_ORDERS_PER_PRIMITIVE + 2..100_000) as DrawOrder,
            };
            let mut case = items(rng, len, spread);
            // The greatest order is the one that decides how they are sorted.
            let greatest = rng.random_range(0..len);
            case[greatest].order = spread - 1;
            if rng.random_ratio(1, 5) {
                case.sort_by_key(|item| item.order);
            }
            cases.push(case);
        }
        cases
    }

    #[test]
    fn draw_order_sort_is_a_stable_sort_by_draw_order() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut permutation = Permutation::default();
        let mut gathered = Vec::new();
        for case in cases(&mut rng) {
            let mut expected = case.clone();
            expected.sort_by_key(|item| item.order);
            let mut sorted = case.clone();
            sort_by_draw_order(
                &mut sorted,
                &mut permutation,
                &mut gathered,
                &mut Vec::new(),
                |item| item.order,
            );
            assert_eq!(sorted, expected, "sorting {case:?}");
        }
    }

    #[test]
    fn draw_order_sort_counts_orders_unless_they_spread_too_wide() {
        let mut rng = StdRng::seed_from_u64(13);
        let mut gathered = Vec::new();
        let (mut counted, mut packed) = (0, 0);
        for mut case in cases(&mut rng) {
            let mut permutation = Permutation::default();
            let in_order = case.is_sorted_by_key(|item| item.order);
            let greatest = case.iter().map(|item| item.order).max().unwrap_or(0) as usize;
            sort_by_draw_order(
                &mut case,
                &mut permutation,
                &mut gathered,
                &mut Vec::new(),
                |item| item.order,
            );
            let (counts, keys) = (permutation.counts.len(), permutation.keys.len());
            if in_order {
                assert_eq!((counts, keys), (0, 0), "sorting what is in order");
            } else if greatest > case.len() * MAX_ORDERS_PER_PRIMITIVE {
                assert_eq!((counts, keys), (0, case.len()), "orders up to {greatest}");
                packed += 1;
            } else {
                assert_eq!((counts, keys), (greatest + 1, 0), "orders up to {greatest}");
                counted += 1;
            }
        }
        assert!(
            counted > 0 && packed > 0,
            "{counted} counted, {packed} packed"
        );
    }

    #[test]
    fn draw_order_sort_counts_orders_up_to_the_widest_spread_it_allows() {
        let mut permutation = Permutation::default();
        let mut gathered = Vec::new();
        let item = |order, painted| Item {
            order,
            texture: 0,
            tile: 0,
            painted,
        };
        let widest = (2 * MAX_ORDERS_PER_PRIMITIVE) as DrawOrder;
        let mut sorted = vec![item(widest, 0), item(0, 1)];
        sort_by_draw_order(
            &mut sorted,
            &mut permutation,
            &mut gathered,
            &mut Vec::new(),
            |item| item.order,
        );
        assert_eq!(sorted, [item(0, 1), item(widest, 0)]);
        assert_eq!(permutation.counts.len(), widest as usize + 1);

        let mut sorted = vec![item(widest + 1, 0), item(0, 1)];
        sort_by_draw_order(
            &mut sorted,
            &mut permutation,
            &mut gathered,
            &mut Vec::new(),
            |item| item.order,
        );
        assert_eq!(sorted, [item(0, 1), item(widest + 1, 0)]);
        assert_eq!(permutation.keys.len(), 2);
    }

    #[test]
    fn sprite_sort_is_a_stable_sort_by_order_texture_and_tile() {
        let mut rng = StdRng::seed_from_u64(11);
        let mut permutation = Permutation::default();
        let mut gathered = Vec::new();
        for case in cases(&mut rng) {
            let mut expected = case.clone();
            expected.sort_by_key(|item| (item.order, item.texture, item.tile));
            let mut sorted = case.clone();
            sort_sprites(
                &mut sorted,
                &mut permutation,
                &mut gathered,
                &mut Vec::new(),
                |item| (item.order, item.texture, item.tile),
            );
            assert_eq!(sorted, expected, "sorting {case:?}");
            sort_sprites(
                &mut sorted,
                &mut permutation,
                &mut gathered,
                &mut Vec::new(),
                |item| (item.order, item.texture, item.tile),
            );
            assert_eq!(sorted, expected, "sorting {case:?} again");
        }
    }

    #[test]
    fn sprite_sort_keeps_every_bit_of_its_keys() {
        let mut permutation = Permutation::default();
        let mut gathered = Vec::new();
        let item = |order, texture, tile, painted| Item {
            order,
            texture,
            tile,
            painted,
        };
        let mut sorted = vec![
            item(u32::MAX, 0, 0, 0),
            item(0, u32::MAX, 0, 1),
            item(0, 0, u32::MAX, 2),
            item(0, 0, 0, 3),
        ];
        sort_sprites(
            &mut sorted,
            &mut permutation,
            &mut gathered,
            &mut Vec::new(),
            |item| (item.order, item.texture, item.tile),
        );
        let painted: Vec<usize> = sorted.iter().map(|item| item.painted).collect();
        assert_eq!(painted, [3, 2, 1, 0]);
    }
}
