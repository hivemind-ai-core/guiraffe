// Added for GUIraffe on top of gpui-fast's retained subtrees and splices.
//! Patching last frame in place.
//!
//! A frame in which one cached view changed is otherwise drawn as a splice
//! (see [`crate::fast::splice`]): the view is built again, and everything
//! around it is copied from last frame, stretch by stretch, into a new frame.
//! Copying is cheap per primitive, but it is still done for every primitive,
//! hitbox, listener and dispatch node of the window, and the copied scene is
//! put in drawing order again.
//!
//! When the view built again comes out the same shape as it was drawn last
//! frame — the same primitives at the same bounds under the same clips, the
//! same hitboxes, dispatch nodes, tab stops and element states — nothing
//! around it can tell it apart from last frame's. Then the window keeps last
//! frame, and the view's new primitives, listeners, dispatch nodes and
//! records take the places of its old ones there. The scene stays in drawing
//! order, since every primitive keeps the draw order it had; the bounds tree,
//! the hitboxes, the tab order and the dispatch tree's indices stand as they
//! are. What the frame costs is the view's, not the window's.
//!
//! Anything else falls back to drawing the frame as before: more than one
//! view to build, a view that is not cached, a change in what anything else
//! read, a moved pointer, focus, scroll, layer, drag, prompt, accessibility,
//! or a view built again that came out another shape (other geometry, text
//! shaped to another number of glyphs, another set of handlers). The view
//! built for the patch is then built again by the frame, as a splice builds
//! it.

use crate::fast::dependencies::state_epoch;
use crate::fast::retained::PaintStatus;
use crate::fast::splice::view_entity;
use crate::window::{DrawPhase, PaintIndex, PrepaintStateIndex};
use crate::{
    App, EntityId, FocusId, GlobalElementId, PaintOperation, Pixels, Point, Primitive, Size, Window,
};
use collections::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::ops::Range;

/// What a patched frame compares against and looks subtrees up by.
pub(crate) struct PatchState {
    /// Off where `GPUI_FRAME_PATCH=0`, to compare against drawing every
    /// frame.
    pub(crate) enabled: bool,
    last: Option<LastFrame>,
    index: Option<PatchIndex>,
    /// The frame a patch builds its view into, swapped in for the window's
    /// next frame while it does, kept from one patch to the next so that it
    /// stays as small as a view and costs little to clear.
    scratch: Option<crate::window::Frame>,
    pub(crate) stats: PatchStats,
}

/// How many frames were patched, and how many could have been but were drawn
/// after all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PatchStats {
    /// Frames drawn by patching last frame.
    pub patched: u64,
    /// Frames that tried to and were drawn as before.
    pub refused: u64,
}

impl PatchState {
    pub(crate) fn new() -> Self {
        PatchState {
            enabled: std::env::var("GPUI_FRAME_PATCH").map_or(true, |value| value != "0"),
            last: None,
            index: None,
            scratch: None,
            stats: PatchStats::default(),
        }
    }
}

/// What the last frame was drawn under.
#[derive(Clone, Copy, PartialEq)]
struct LastFrame {
    global_generation: u64,
    state_epoch: u64,
    mouse_position: Point<Pixels>,
    viewport_size: Size<Pixels>,
    scale_factor: f32,
    focus: Option<FocusId>,
    active: bool,
}

impl LastFrame {
    fn of(window: &Window, cx: &App) -> Self {
        LastFrame {
            global_generation: cx.dependencies.global_generation(),
            state_epoch: state_epoch(),
            mouse_position: window.mouse_position,
            viewport_size: window.viewport_size,
            scale_factor: window.scale_factor(),
            focus: window.focus,
            active: window.active.get(),
        }
    }
}

/// The records of the frame a patch draws on, looked up by what they read
/// and by the view they stand for. Built the first time a frame is patched
/// and kept while frames are patched, since a patch leaves the records where
/// they are.
struct PatchIndex {
    /// The records that read each entity themselves.
    readers: FxHashMap<EntityId, SmallVec<[u32; 2]>>,
    /// The record of each view.
    views: FxHashMap<EntityId, u32>,
    /// The record each one is nested in, `u32::MAX` for those nested in none.
    parents: Vec<u32>,
}

impl PatchIndex {
    fn build(records: &[crate::fast::retained::RetainedSubtree]) -> Self {
        let mut readers: FxHashMap<EntityId, SmallVec<[u32; 2]>> = FxHashMap::default();
        let mut views = FxHashMap::default();
        let mut parents = Vec::with_capacity(records.len());
        // The records open at each point, with the last record nested in each.
        let mut open: Vec<(u32, usize)> = Vec::new();
        for (index, record) in records.iter().enumerate() {
            while open.last().is_some_and(|&(_, last)| index > last) {
                open.pop();
            }
            parents.push(open.last().map_or(u32::MAX, |&(parent, _)| parent));
            open.push((index as u32, index + record.nested));
            if let Some(entity) = view_entity(&record.id) {
                views.insert(entity, index as u32);
            }
            for entity in record.own_dependencies.entities.iter() {
                let readers = readers.entry(*entity).or_default();
                if readers.last() != Some(&(index as u32)) {
                    readers.push(index as u32);
                }
            }
        }
        PatchIndex {
            readers,
            views,
            parents,
        }
    }

    /// The records `record` is nested in, innermost first.
    fn ancestors(&self, record: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(record as u32), |&index| {
            let parent = self.parents[index as usize];
            (parent != u32::MAX).then_some(parent)
        })
        .skip(1)
        .map(|index| index as usize)
    }
}

/// Notes what a frame was drawn under, once it is drawn, for the next one
/// to be patched on it. `start` is where the change log stood as the frame
/// began: what came before, the frame accounted for. A frame drawn in full
/// leaves new records, which the index has to be built again for.
pub(crate) fn finish_frame(window: &mut Window, cx: &mut App, start: usize, patched: bool) {
    cx.entities.access_log.patch_log.drop_before(start);
    let last = LastFrame::of(window, cx);
    let state = &mut window.retained_state.patch;
    state.last = Some(last);
    if !patched {
        state.index = None;
    }
}

/// Draws this frame by patching last frame, if it can be. See the module
/// documentation. Returns whether it did; if not, nothing it did has to be
/// undone before the frame is drawn as before.
pub(crate) fn try_patch_frame(window: &mut Window, cx: &mut App) -> bool {
    let state = &window.retained_state.patch;
    if !state.enabled {
        return false;
    }
    let Some(last) = state.last else {
        return false;
    };
    if !eligible(window, cx, last) {
        return false;
    }
    let patched = find_gap(window, cx).is_some_and(|gap| patch_gap(window, gap, cx));
    let stats = &mut window.retained_state.patch.stats;
    if patched {
        stats.patched += 1;
    } else {
        stats.refused += 1;
    }
    patched
}

/// Whether nothing about the window rules out patching: nothing that is not
/// an entity read by a retained subtree changed since last frame.
fn eligible(window: &mut Window, cx: &App, last: LastFrame) -> bool {
    window.a11y.sync_active_flag();
    let frame = &window.rendered_frame;
    !window.refreshing
        && window.retained_state.view_retention
        && !cx.has_active_drag()
        && !window.a11y.is_active()
        && !window.is_inspector_picking(cx)
        && !window.has_prompt()
        && window.fast_layers.layers.is_empty()
        && !window.fast_layers.scrolls.any_scrolled()
        && window.retained_state.dirty_subtrees.is_empty()
        && window.retained_state.prebuilt.is_empty()
        && frame.deferred_draws.is_empty()
        && frame.scene.layers.frames.is_empty()
        && crate::fast::composition::no_surfaces(window, frame, false)
        && frame.focus == window.focus
        && frame.window_active == window.active.get()
        && LastFrame::of(window, cx) == last
}

/// The record of the one view to build again, if exactly one cached view
/// read something that changed, or was notified, and every dirty view is it
/// or a view around it.
fn find_gap(window: &mut Window, cx: &App) -> Option<usize> {
    let changes: SmallVec<[EntityId; 16]> = cx
        .entities
        .access_log
        .patch_log
        .entries()?
        .iter()
        .copied()
        .collect();
    if window.retained_state.patch.index.is_none() {
        window.retained_state.patch.index =
            Some(PatchIndex::build(&window.rendered_frame.retained.records));
    }
    let index = window.retained_state.patch.index.as_ref()?;
    let records = &window.rendered_frame.retained.records;
    let notified = &window.retained_state.notified_entities;
    let drawn_views = &window.rendered_frame.dispatch_tree.view_node_ids;

    let mut candidates: SmallVec<[u32; 8]> = SmallVec::new();
    for entity in &changes {
        if let Some(readers) = index.readers.get(entity) {
            candidates.extend_from_slice(readers);
        }
    }
    for entity in notified {
        match index.views.get(entity) {
            Some(&record) => candidates.push(record),
            // A view drawn without a record of its own cannot be patched.
            None if drawn_views.contains_key(entity) => return None,
            None => {}
        }
    }
    candidates.sort_unstable();
    candidates.dedup();

    let mut leaves: SmallVec<[usize; 2]> = SmallVec::new();
    for &candidate in &candidates {
        let record = &records[candidate as usize];
        // As `Window::mark_changed_retained_views_dirty` decides it, but
        // taking every update for a change, as inside a notified view.
        let changed = view_entity(&record.id).is_some_and(|entity| notified.contains(&entity))
            || if record.is_cached() {
                cx.notified_dependencies_changed(&record.own_dependencies)
            } else {
                cx.dependencies_changed(&record.own_dependencies, true)
            }
            || crate::fast::layers::invalidate::offset_read_changed(
                window,
                &record.own_dependencies,
            );
        if changed {
            leaves.push(candidate as usize);
        }
    }
    // A view inside one built again is built with it.
    let outermost: SmallVec<[usize; 2]> = leaves
        .iter()
        .copied()
        .filter(|&leaf| {
            !index
                .ancestors(leaf)
                .any(|ancestor| leaves.contains(&ancestor))
        })
        .collect();
    let [gap] = outermost[..] else {
        return None;
    };

    let mut around: FxHashSet<EntityId> = index
        .ancestors(gap)
        .filter_map(|ancestor| view_entity(&records[ancestor].id))
        .collect();
    around.extend(view_entity(&records[gap].id));
    // Every dirty view is the gap, one around it, or not a view drawn here.
    window
        .dirty_views
        .iter()
        .all(|view| around.contains(view) || !drawn_views.contains_key(view))
        .then_some(gap)
}

/// Builds the view of last frame's record `gap` again and, if it comes out
/// the shape it had, patches it into last frame. Returns whether it did.
fn patch_gap(window: &mut Window, gap: usize, cx: &mut App) -> bool {
    let Some(old) = OldGap::of(window, gap) else {
        return false;
    };
    let hitboxes = old.prepaint.start.hitboxes_index..old.prepaint.end.hitboxes_index;
    let Some(resume_hitbox_ids) = window.reuse_hitbox_ids(hitboxes) else {
        return false;
    };
    let entity_updates = cx.entities.access_log.update_generation();
    let mut scratch = window
        .retained_state
        .patch
        .scratch
        .take()
        .unwrap_or_else(|| Window::new_patch_frame(cx));
    std::mem::swap(&mut window.next_frame, &mut scratch);
    window.global_ids.begin_patch();
    let claims = window.record_claimed_layout_keys();
    if let Some(entity) = view_entity(&old.id) {
        window.dirty_views.insert(entity);
    }
    let built = build_gap(window, gap, &old, cx);
    let claimed = window.finish_recording_claimed_layout_keys(claims);
    window.resume_hitbox_ids(resume_hitbox_ids);
    window.invalidator.set_phase(DrawPhase::None);

    let patched = built && same_shape(window, gap, &old);
    if patched {
        commit(window, gap, &old, entity_updates, cx);
        window
            .layout_engine
            .as_mut()
            .unwrap()
            .finish_patched_frame(&claimed);
    } else {
        crate::fast::layout::give_back_patched_claims(
            window.layout_engine.as_mut().unwrap(),
            &claimed,
            false,
        );
        // The states the view took go back for the frame to take again.
        let states = std::mem::take(&mut window.next_frame.element_states);
        window.rendered_frame.element_states.extend(states);
    }
    window.global_ids.end_patch(patched);
    window.next_frame.clear();
    std::mem::swap(&mut window.next_frame, &mut scratch);
    window.retained_state.patch.scratch = Some(scratch);
    patched
}

/// What last frame drew of the view to patch, and where.
struct OldGap {
    id: GlobalElementId,
    nested: usize,
    prepaint: Range<PrepaintStateIndex>,
    paint: Range<PaintIndex>,
    /// The ids of the records it holds, itself first.
    ancestors: SmallVec<[GlobalElementId; 4]>,
}

impl OldGap {
    fn of(window: &Window, gap: usize) -> Option<Self> {
        let frame = &window.rendered_frame;
        let record = &frame.retained.records[gap];
        let (prepaint, paint) = (&record.prepaint_range, &record.paint_range);
        let index = window.retained_state.patch.index.as_ref()?;
        let can = record
            .rebuild
            .as_ref()
            .is_some_and(|rebuild| rebuild.is_cached())
            && matches!(record.paint, PaintStatus::Painted { .. })
            && record.context.composition_surface.is_none()
            && prepaint.start.tooltips_index == prepaint.end.tooltips_index
            && prepaint.start.deferred_draws_index == prepaint.end.deferred_draws_index
            && prepaint.start.fast_a11y_index == prepaint.end.fast_a11y_index
            && paint.start.input_handlers_index == paint.end.input_handlers_index
            && paint.start.fast == paint.end.fast
            && frame.scene.paint_operations[paint.start.scene_index..paint.end.scene_index]
                .iter()
                .all(patchable);
        if !can {
            return None;
        }
        let mut ancestors: SmallVec<[GlobalElementId; 4]> = index
            .ancestors(gap)
            .map(|ancestor| frame.retained.records[ancestor].id.clone())
            .collect();
        ancestors.reverse();
        Some(OldGap {
            id: record.id.clone(),
            nested: record.nested,
            prepaint: prepaint.clone(),
            paint: paint.clone(),
            ancestors,
        })
    }
}

/// Whether a paint operation can be patched where it lies: a primitive, but
/// not a path, whose id is its place, nor a surface; or a layer.
fn patchable(operation: &PaintOperation) -> bool {
    match operation {
        PaintOperation::Primitive(primitive) => patchable_primitive(primitive),
        // A layer drawn at the bounds it had keeps its draw order, which the
        // primitives in it share.
        PaintOperation::StartLayer(_) | PaintOperation::EndLayer => true,
        PaintOperation::FastRoundedClip(_) => false,
    }
}

/// See [`patchable`].
fn patchable_primitive(primitive: &Primitive) -> bool {
    !matches!(primitive, Primitive::Path(_) | Primitive::Surface(_))
}

/// Builds the view of last frame's record `gap` again into the window's next
/// frame, which is empty, as a splice builds it in its gap: where it was, at
/// the bounds it had, with what it inherited there. Returns whether it was
/// built.
fn build_gap(window: &mut Window, gap: usize, old: &OldGap, cx: &mut App) -> bool {
    let record = &window.rendered_frame.retained.records[gap];
    let (Some(rebuild), context) = (record.rebuild.clone(), record.context.clone()) else {
        return false;
    };
    let Some(mut built) = window.cached_gap(gap, cx) else {
        return false;
    };
    let Some(bounds) = built.bounds else {
        return false;
    };
    window
        .retained_state
        .subtree_stack
        .extend(old.ancestors.iter().cloned());
    window.invalidator.set_phase(DrawPhase::Prepaint);
    // The node the view hung off last frame, which its first node is moved
    // back under when it is patched in.
    let parent = window.next_frame.dispatch_tree.push_node();
    let inherited = window.enter_gap(
        &built.global_id,
        &rebuild,
        context.content_mask,
        context.opacity,
    );
    window.element_id_stack.push(built.element_id());
    let scope = crate::fast::layout_key::enter_prepaint_scope(window, built.layout_key);
    let mut prepaint = crate::fast::retained::prepaint_view(
        &mut built.view,
        Some(&built.global_id),
        bounds,
        &mut built.layout,
        window,
        cx,
    );
    crate::fast::layout_key::exit_prepaint_scope(window, scope);
    window.next_frame.dispatch_tree.pop_node();
    window.element_id_stack.pop();
    window.leave_gap(inherited);

    window.invalidator.set_phase(DrawPhase::Paint);
    let inherited = window.enter_gap(
        &built.global_id,
        &rebuild,
        context.content_mask,
        context.opacity,
    );
    window.element_id_stack.push(built.element_id());
    window.next_frame.dispatch_tree.set_active_node(parent);
    crate::fast::retained::paint_view(
        &mut built.view,
        Some(&built.global_id),
        &mut prepaint,
        window,
        cx,
    );
    window.element_id_stack.pop();
    window.leave_gap(inherited);
    let depth = window.retained_state.subtree_stack.len() - old.ancestors.len();
    window.retained_state.subtree_stack.truncate(depth);
    true
}

/// Whether what was built into the next frame stands for what last frame
/// drew of the view `old`, slot for slot: patched in where that lies, nothing
/// around it would draw, hit-test, focus or dispatch differently.
fn same_shape(window: &mut Window, gap: usize, old: &OldGap) -> bool {
    let new = &window.next_frame;
    let frame = &window.rendered_frame;
    let records = &new.retained.records;
    let old_records = &frame.retained.records[gap..];
    if records.len() != old.nested + 1
        || records.iter().zip(old_records).any(|(new, old)| {
            new.id != old.id
                || !matches!(new.paint, PaintStatus::Painted { .. })
                || new.nested != old.nested
                || new.hover_dependencies != old.hover_dependencies
                || new.own_hovers != old.own_hovers
                || *new.dependencies.entities != *old.dependencies.entities
                || *new.own_dependencies.entities != *old.own_dependencies.entities
        })
    {
        return false;
    }
    let record = &records[0];
    let prepaint = &record.prepaint_range;
    // Its prepaint went first, after the node it hangs off; its paint came
    // straight after.
    let lengths_match = |new: Range<usize>, old: Range<usize>| new.len() == old.len();
    if !lengths_match(
        prepaint.start.hitboxes_index..prepaint.end.hitboxes_index,
        old.prepaint.start.hitboxes_index..old.prepaint.end.hitboxes_index,
    ) || !lengths_match(
        prepaint.start.dispatch_tree_index..prepaint.end.dispatch_tree_index,
        old.prepaint.start.dispatch_tree_index..old.prepaint.end.dispatch_tree_index,
    ) || new.tooltip_requests.len() + new.deferred_draws.len() + new.input_handlers.len() != 0
        || new.window_control_hitboxes.len() != 0
        || !crate::fast::composition::no_surfaces(window, new, true)
        || !lengths_match(
            0..new.mouse_listeners.len(),
            old.paint.start.mouse_listeners_index..old.paint.end.mouse_listeners_index,
        )
    {
        return false;
    }

    // Hitboxes: the same ids at the same bounds, so nothing hit-tests or
    // hovers differently.
    let old_hitboxes =
        &frame.hitboxes[old.prepaint.start.hitboxes_index..old.prepaint.end.hitboxes_index];
    if new.hitboxes.len() != old_hitboxes.len()
        || new.hitboxes.iter().zip(old_hitboxes).any(|(new, old)| {
            new.id != old.id
                || new.bounds != old.bounds
                || new.content_mask != old.content_mask
                || new.behavior != old.behavior
        })
    {
        return false;
    }

    // Cursor styles, the element states it used and its tab stops, as they
    // were.
    let old_cursors = &frame.cursor_styles
        [old.paint.start.cursor_styles_index..old.paint.end.cursor_styles_index];
    if new.cursor_styles.len() != old_cursors.len()
        || new
            .cursor_styles
            .iter()
            .zip(old_cursors)
            .any(|(new, old)| new.hitbox_id != old.hitbox_id || new.style != old.style)
    {
        return false;
    }
    let old_states = frame.accessed_element_states[old.prepaint.start.accessed_element_states_index
        ..old.prepaint.end.accessed_element_states_index]
        .iter()
        .chain(
            &frame.accessed_element_states[old.paint.start.accessed_element_states_index
                ..old.paint.end.accessed_element_states_index],
        );
    if new.accessed_element_states.len()
        != (old.prepaint.end.accessed_element_states_index
            - old.prepaint.start.accessed_element_states_index)
            + (old.paint.end.accessed_element_states_index
                - old.paint.start.accessed_element_states_index)
        || !new.accessed_element_states.iter().eq(old_states)
    {
        return false;
    }
    if !new.tab_stops.same_insertions(
        &frame.tab_stops.insertion_history
            [old.paint.start.tab_handle_index..old.paint.end.tab_handle_index],
    ) {
        return false;
    }

    // Dispatch nodes: the same tree under the node it hangs off, with the
    // same focusable elements and views at the same places.
    let old_start = old.prepaint.start.dispatch_tree_index;
    let old_nodes = &frame.dispatch_tree.nodes;
    let hung_off = old_nodes[old_start].parent;
    let new_nodes = &new.dispatch_tree.nodes;
    if new_nodes.len() != 1 + (old.prepaint.end.dispatch_tree_index - old_start) {
        return false;
    }
    for (offset, node) in new_nodes[1..].iter().enumerate() {
        let old_node = &old_nodes[old_start + offset];
        let parent = node.parent.map(|parent| {
            if parent.0 == 0 {
                hung_off
            } else {
                Some(crate::key_dispatch::DispatchNodeId(
                    old_start + parent.0 - 1,
                ))
            }
        });
        if parent.flatten() != old_node.parent
            || node.focus_id != old_node.focus_id
            || node.view_id != old_node.view_id
        {
            return false;
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    if new.debug_bounds_records.len()
        != old.paint.end.debug_bounds_index - old.paint.start.debug_bounds_index
    {
        return false;
    }
    same_primitives(window, old)
}

/// Whether the primitives built into the next frame can take the places of
/// those last frame drew of the view `old`: the same kinds under the same
/// clips, each keeping its draw order. A primitive in a layer takes the
/// layer's, wherever it lies in it. A layer, or a primitive outside any, takes
/// its draw order from what it meets: it may move only where it meets what it
/// met, which its bounds tree entry is moved to check (see
/// [`crate::fast::bounds_tree::BoundsTree::patch`]). Last of the checks, as
/// it moves them; if it fails, it moves them back.
fn same_primitives(window: &mut Window, old: &OldGap) -> bool {
    let range = old.paint.start.scene_index..old.paint.end.scene_index;
    let new = &window.next_frame.scene.paint_operations;
    let scene = &mut window.rendered_frame.scene;
    if new.len() != range.len() {
        return false;
    }
    let Some((_, insertions)) = crate::fast::scene::SceneSlots::operations(scene) else {
        return false;
    };
    let insertions = insertions[range.clone()].to_vec();
    let mut moved: SmallVec<[(usize, crate::Bounds<crate::ScaledPixels>); 4]> = SmallVec::new();
    let mut fits = true;
    for ((new, old), insertion) in new
        .iter()
        .zip(&scene.paint_operations[range])
        .zip(insertions)
    {
        let (new_bounds, old_bounds) = match (new, old) {
            (PaintOperation::Primitive(new), PaintOperation::Primitive(old)) => {
                let clipped = |primitive: &Primitive| {
                    primitive
                        .bounds()
                        .intersect(&primitive.content_mask().bounds)
                };
                if !patchable_primitive(new)
                    || crate::fast::scene::primitive_kind(new)
                        != crate::fast::scene::primitive_kind(old)
                    || new.content_mask() != old.content_mask()
                    || clipped(new).is_empty()
                {
                    fits = false;
                    break;
                }
                (clipped(new), clipped(old))
            }
            (PaintOperation::StartLayer(new), PaintOperation::StartLayer(old)) => (*new, *old),
            (PaintOperation::EndLayer, PaintOperation::EndLayer) => continue,
            _ => {
                fits = false;
                break;
            }
        };
        if insertion == u32::MAX || new_bounds == old_bounds {
            continue;
        }
        if !scene.primitive_bounds.patch(insertion as usize, new_bounds) {
            fits = false;
            break;
        }
        moved.push((insertion as usize, old_bounds));
    }
    if !fits {
        for (insertion, bounds) in moved.into_iter().rev() {
            scene.primitive_bounds.unpatch(insertion, bounds);
        }
    }
    fits
}

/// Puts what was built into the next frame where last frame drew the view
/// `old`, which [`same_shape`] found it can take the place of.
fn commit(window: &mut Window, gap: usize, old: &OldGap, entity_updates: u64, cx: &mut App) {
    let writes_now = cx.entities.write_generation();
    let new = &mut window.next_frame;
    let frame = &mut window.rendered_frame;

    // Primitives, each where its old one lies, at its old one's draw order.
    let scene_start = old.paint.start.scene_index;
    let slots = crate::fast::scene::SceneSlots::operations(&mut frame.scene)
        .map(|(slots, _)| slots[scene_start..old.paint.end.scene_index].to_vec())
        .unwrap_or_default();
    for (offset, operation) in new.scene.paint_operations.drain(..).enumerate() {
        let at = scene_start + offset;
        // A layer keeps its draw order where it moved: see `same_primitives`.
        let PaintOperation::Primitive(mut primitive) = operation else {
            frame.scene.paint_operations[at] = operation;
            continue;
        };
        let PaintOperation::Primitive(old_primitive) = &frame.scene.paint_operations[at] else {
            continue;
        };
        set_order(&mut primitive, order(old_primitive));
        place(&mut frame.scene, slots[offset] as usize, &primitive);
        frame.scene.paint_operations[at] = PaintOperation::Primitive(primitive);
    }

    // Listeners and cursor styles take the old ones' places.
    let listeners = old.paint.start.mouse_listeners_index;
    for (offset, listener) in new.mouse_listeners.drain(..).enumerate() {
        frame.mouse_listeners[listeners + offset] = listener;
    }
    #[cfg(any(test, feature = "test-support"))]
    {
        let start = old.paint.start.debug_bounds_index;
        for (offset, (selector, bounds)) in new.debug_bounds_records.drain(..).enumerate() {
            frame.debug_bounds.insert(selector.clone(), bounds);
            frame.debug_bounds_records[start + offset] = (selector, bounds);
        }
    }

    // Dispatch nodes, moved back under the node the view hung off.
    let old_start = old.prepaint.start.dispatch_tree_index;
    let hung_off = frame.dispatch_tree.nodes[old_start].parent;
    for (offset, mut node) in new.dispatch_tree.nodes.drain(..).skip(1).enumerate() {
        node.parent = match node.parent {
            Some(parent) if parent.0 == 0 => hung_off,
            Some(parent) => Some(crate::key_dispatch::DispatchNodeId(
                old_start + parent.0 - 1,
            )),
            None => None,
        };
        frame.dispatch_tree.nodes[old_start + offset] = node;
    }

    // Element states, back with the rest.
    frame.element_states.extend(new.element_states.drain());

    // Records, at their old ranges, which every slot kept.
    let first = gap;
    let mut line_slots = Vec::with_capacity(2 * (old.nested + 1));
    for (offset, mut record) in new.retained.records.drain(..).enumerate() {
        let old_record = &mut frame.retained.records[first + offset];
        record.prepaint_range = old_record.prepaint_range.clone();
        record.paint_range = old_record.paint_range.clone();
        record.paint = PaintStatus::Painted { source: None };
        if offset == 0 {
            line_slots.push(
                record.prepaint_range.start.line_layout_index.clone()
                    ..record.prepaint_range.end.line_layout_index.clone(),
            );
            line_slots.push(
                record.paint_range.start.line_layout_index.clone()
                    ..record.paint_range.end.line_layout_index.clone(),
            );
        }
        *old_record = record;
    }
    window.text_system().finish_patched_frame(&line_slots);

    // The views around it are up to date with everything written and
    // updated so far: what changed in them is the view patched.
    let index = window.retained_state.patch.index.as_ref();
    for ancestor in index.into_iter().flat_map(|index| index.ancestors(gap)) {
        let record = &mut window.rendered_frame.retained.records[ancestor];
        let mut dependencies = record.dependencies.written_up_to(writes_now);
        dependencies.updates = entity_updates;
        record.dependencies = dependencies;
    }
}

fn order(primitive: &Primitive) -> crate::DrawOrder {
    match primitive {
        Primitive::Shadow(shadow) => shadow.order,
        Primitive::Quad(quad) => quad.order,
        Primitive::Path(path) => path.order,
        Primitive::Underline(underline) => underline.order,
        Primitive::MonochromeSprite(sprite) => sprite.order,
        Primitive::SubpixelSprite(sprite) => sprite.order,
        Primitive::PolychromeSprite(sprite) => sprite.order,
        Primitive::Surface(surface) => surface.order,
    }
}

fn set_order(primitive: &mut Primitive, order: crate::DrawOrder) {
    match primitive {
        Primitive::Shadow(shadow) => shadow.order = order,
        Primitive::Quad(quad) => quad.order = order,
        Primitive::Path(path) => path.order = order,
        Primitive::Underline(underline) => underline.order = order,
        Primitive::MonochromeSprite(sprite) => sprite.order = order,
        Primitive::SubpixelSprite(sprite) => sprite.order = order,
        Primitive::PolychromeSprite(sprite) => sprite.order = order,
        Primitive::Surface(surface) => surface.order = order,
    }
}

/// Puts `primitive` at `slot` of its kind's list in `scene`.
fn place(scene: &mut crate::Scene, slot: usize, primitive: &Primitive) {
    match primitive {
        Primitive::Shadow(shadow) => scene.shadows[slot] = *shadow,
        Primitive::Quad(quad) => scene.quads[slot] = *quad,
        Primitive::Path(path) => scene.paths[slot] = path.clone(),
        Primitive::Underline(underline) => scene.underlines[slot] = *underline,
        Primitive::MonochromeSprite(sprite) => scene.monochrome_sprites[slot] = *sprite,
        Primitive::SubpixelSprite(sprite) => scene.subpixel_sprites[slot] = *sprite,
        Primitive::PolychromeSprite(sprite) => scene.polychrome_sprites[slot] = *sprite,
        Primitive::Surface(surface) => scene.surfaces[slot] = surface.clone(),
    }
}
