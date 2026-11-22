//! [`DataUi`]: the host component that instantiates a document anywhere in a static UI.
//!
//! **Granular re-rendering:** every node whose props read a slot is rendered as its own small
//! entity ([`BoundNode`]). A patch notifies only the entities bound to the slots that changed;
//! the rest of the document is untouched (and, on gpui-fast, not even revisited).

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Window, div, px, uniform_list,
};

use crate::{
    ItemContext, Payload,
    ir::Document,
    payload::{ChangeTarget, InputRequest},
    position::Position,
    repeat::KeyedList,
    slots::{Patch, PatchBuf, SlotId, SlotTable},
    template::{CompileError, EventId, Names, Template},
    value::Value,
    widget::{Emit, NodeView, Registry},
};

/// Application logic behind a document: events in, slot patches out.
/// Native Rust today; the same message-shaped boundary can later be a wasm component.
pub trait Logic: 'static {
    /// Called after every (re)compile: cache slot/event ids by name, and push data for any
    /// slots that need it (a hot reload may add slots).
    fn bind(&mut self, names: &Names, out: &mut PatchBuf);
    /// Called once, after the first bind, to populate initial view data.
    fn init(&mut self, _out: &mut PatchBuf) {}
    fn on_event(&mut self, _event: EventId, _payload: &Value, _out: &mut PatchBuf) {}
    /// Compact input hook. The default preserves existing Value-based logic.
    /// The optimistic value is already in the slot table. Use out.veto() or emit
    /// a replacement patch to reject or normalize a change.
    fn on_payload(&mut self, event: EventId, payload: &Payload, out: &mut PatchBuf) {
        self.on_event(event, &payload.value(), out);
    }
}

/// A no-op logic (static documents).
pub struct NoLogic;

impl Logic for NoLogic {
    fn bind(&mut self, _: &Names, _: &mut PatchBuf) {}
    fn on_event(&mut self, _: EventId, _: &Value, _: &mut PatchBuf) {}
}

/// State shared by the host and its bound-node entities (UI thread only).
struct Repeater {
    model: RefCell<KeyedList<Rc<ItemScope>>>,
    slot: SlotId,
    node: u32,
    parent: Option<std::rc::Weak<ItemScope>>,
    key_path: Arc<[gpui_kit::SharedString]>,
}
type RepeatState = Rc<Repeater>;
static NEXT_WIDGET_NAMESPACE: AtomicU64 = AtomicU64::new(1);

struct Shared {
    widget_namespace: Cell<u64>,
    template: RefCell<Arc<Template>>,
    slots: RefCell<SlotTable>,
    generation: Cell<u32>,
    emit: Emit,
    bound: RefCell<BTreeMap<(u32, u64), Entity<BoundNode>>>,
    repeats: RefCell<BTreeMap<(u32, u64), RepeatState>>,
    next_item: Cell<u64>,
    items: RefCell<HashMap<u64, std::rc::Weak<ItemScope>>>,
    reader: crate::controls::PropsRead,
    input_emit: crate::widget::InputEmit,
    controls: crate::controls::ControlCache,

    renders: RefCell<Vec<u64>>,
    logic: RefCell<Box<dyn Logic>>,
    buf: RefCell<PatchBuf>,
    changed: RefCell<Vec<SlotId>>,
    events_handled: Cell<u64>,
    stale_events: Cell<u64>,
    host_renders: Cell<u64>,
    repeat_viewport: Cell<crate::RepeatViewport>,
    owner: std::rc::Weak<Shared>,
}

impl Shared {
    /// Event path. Deliberately does **not** update the host entity: on gpui-fast an entity
    /// write invalidates its whole retained subtree, so only bound nodes are notified.
    fn valid_event(&self, event: EventId, generation: u32) -> bool {
        if generation != self.generation.get() {
            self.stale_events.set(self.stale_events.get() + 1);
            return false;
        }
        self.template.borrow().names().event_name(event).is_some()
    }
    fn item_context(&self, scope: u64) -> Option<ItemContext> {
        let item = self.items.borrow().get(&scope)?.upgrade()?;
        if !item.active.get() {
            return None;
        }
        let parent = item
            .parent
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
            .and_then(|parent| self.item_context(parent.id))
            .map(Arc::new);
        Some(ItemContext {
            slot: item.slot,
            key: item.value.borrow().field(&item.key_path).clone(),
            key_path: item.key_path.clone(),
            index: item.position.index()?,
            identity: scope,
            repeat: item.template_node,
            parent,
        })
    }
    fn scoped(
        &self,
        payload: Payload,
        scope: u64,
        change: Option<Arc<[gpui_kit::SharedString]>>,
    ) -> Option<Payload> {
        if scope == 0 {
            Some(payload)
        } else {
            Some(Payload::Scoped(Arc::new(crate::payload::ScopedPayload {
                item: self.item_context(scope)?,
                input: payload,
                change,
            })))
        }
    }
    fn dispatch(&self, event: EventId, payload: Value, scope: u64, generation: u32, cx: &mut App) {
        if !self.valid_event(event, generation) {
            return;
        }
        let Some(payload) = self.scoped(Payload::Value(payload), scope, None) else {
            return;
        };
        self.events_handled.set(self.events_handled.get() + 1);
        self.logic
            .borrow_mut()
            .on_payload(event, &payload, &mut self.buf.borrow_mut());
        if self.buf.borrow_mut().take_veto() {
            self.buf.borrow_mut().clear();
        }
        self.flush(cx);
    }

    fn patch(&self, f: impl FnOnce(&Names, &mut PatchBuf), cx: &mut App) {
        let template = self.template.borrow().clone();
        f(template.names(), &mut self.buf.borrow_mut());
        self.flush(cx);
    }

    fn dispatch_input(&self, request: InputRequest, cx: &mut App) {
        let InputRequest {
            event,
            target,
            scope,
            payload,
            generation,
        } = request;
        // No data writes (including optimism) precede this validation.
        if generation != self.generation.get() {
            self.stale_events.set(self.stale_events.get() + 1);
            return;
        }
        if event.is_some_and(|event| !self.valid_event(event, generation)) {
            return;
        }
        let path = match &target {
            Some(ChangeTarget::Item(path)) => Some(path.clone()),
            _ => None,
        };
        let Some(payload) = self.scoped(payload, scope, path) else {
            return;
        };
        let (optimistic, rollback) = match target {
            Some(ChangeTarget::Slot(slot)) => (
                Some(Patch::Set(slot, payload.value())),
                Some(Patch::Set(slot, self.slots.borrow().get(slot).clone())),
            ),
            Some(ChangeTarget::Item(_)) => {
                let Some(change) = payload.item_change() else {
                    return;
                };
                let Some(row) = self
                    .slots
                    .borrow()
                    .get(change.item.slot)
                    .list_get(change.item.index)
                    .cloned()
                else {
                    return;
                };
                let rollback = Patch::Splice(
                    change.item.slot,
                    change.item.index..change.item.index + 1,
                    Arc::from([row]),
                );
                (Some(Patch::SetItem(Arc::new(change))), Some(rollback))
            }
            None => (None, None),
        };
        self.buf.borrow_mut().clear();
        if let Some(optimistic) = optimistic {
            self.buf.borrow_mut().push(optimistic);
            self.flush(cx);
        }
        if let Some(event) = event {
            self.events_handled.set(self.events_handled.get() + 1);
            self.logic
                .borrow_mut()
                .on_payload(event, &payload, &mut self.buf.borrow_mut());
        }
        // Veto rejects the whole event response, including any normalization patches.
        // Optimism and rollback happen synchronously before deferred control sync/draw.
        if self.buf.borrow_mut().take_veto() {
            self.buf.borrow_mut().clear();
            if let Some(rollback) = rollback {
                self.buf.borrow_mut().push(rollback);
            }
        }
        self.flush(cx);
    }

    /// Process patches in order, resolving scoped identities against the current model
    /// after each preceding edit. A stale/deleted identity cannot write another row.
    fn flush(&self, cx: &mut App) {
        if self.buf.borrow().is_empty() {
            return;
        }
        let mut pending = PatchBuf::default();
        std::mem::swap(&mut pending, &mut self.buf.borrow_mut());
        let mut changed = self.changed.borrow_mut();
        changed.clear();
        let mut edits = Vec::new();
        let mut one = PatchBuf::default();
        let mut structural = std::collections::HashSet::new();
        let template = self.template.borrow().clone();
        for mut patch in pending.drain() {
            if let Patch::SetItem(change) = &mut patch {
                let change = Arc::make_mut(change);
                let Some(current) = self.item_context(change.item.identity) else {
                    continue;
                };
                if current.slot != change.item.slot || current.key != change.item.key {
                    continue;
                }
                change.item = current;
            }
            one.push(patch);
            edits.clear();
            self.slots
                .borrow_mut()
                .apply_detailed(&mut one, &mut changed, &mut edits);
            for edit in edits.drain(..) {
                let slot = match &edit {
                    Patch::Set(id, _) | Patch::Splice(id, _, _) => *id,
                    Patch::SetItem(_) => continue,
                };
                let repeats = self.repeats.borrow();
                let targets: Vec<_> = template
                    .slot_nodes(slot)
                    .iter()
                    .filter(|node| {
                        template
                            .node(**node)
                            .is_some_and(|spec| spec.each == Some(slot))
                    })
                    .flat_map(|node| {
                        repeats
                            .range((*node, 0)..=(*node, u64::MAX))
                            .map(|(key, state)| (*key, state.clone()))
                    })
                    .collect();
                drop(repeats);
                for (key, state) in targets {
                    let dirty = match &edit {
                        Patch::Set(_, value) => {
                            let values: Arc<[Value]> = (0..value.list_len().unwrap_or(0))
                                .filter_map(|i| value.list_get(i).cloned())
                                .collect();
                            let len = state.model.borrow().rows.len();
                            self.splice_rows(&state, 0..len, &values, cx)
                        }
                        Patch::Splice(_, range, values) => {
                            self.splice_rows(&state, range.clone(), values, cx)
                        }
                        Patch::SetItem(_) => false,
                    };
                    if dirty {
                        structural.insert(key);
                    }
                }
            }
        }
        std::mem::swap(&mut pending, &mut self.buf.borrow_mut());
        let mut targets = std::collections::HashSet::new();
        for slot in changed.iter() {
            for index in template.slot_nodes(*slot) {
                self.controls.schedule_node(*index, cx);
                let Some(spec) = template.node(*index) else {
                    continue;
                };
                for ((_, scope), entity) in
                    self.bound.borrow().range((*index, 0)..=(*index, u64::MAX))
                {
                    let only_each = spec.each == Some(*slot)
                        && spec.when != Some(*slot)
                        && !spec
                            .props
                            .iter()
                            .any(|prop| matches!(prop,crate::Prop::Slot(id) if id==slot));
                    if !only_each || structural.contains(&(*index, *scope)) {
                        targets.insert(entity.entity_id());
                    }
                }
            }
        }
        drop(changed);
        for target in targets {
            cx.notify(target);
        }
    }

    fn splice_rows(
        &self,
        state: &RepeatState,
        range: std::ops::Range<usize>,
        values: &Arc<[Value]>,
        cx: &mut App,
    ) -> bool {
        // Stored rows carry weak host pointers; creating or dropping a row never writes DataUi.
        let owner = self.owner.clone();
        let app = RefCell::new(cx);
        state.model.borrow_mut().splice(
            range,
            values,
            |value, position| {
                let id = self.next_item.get();
                self.next_item.set(id + 1);
                let item = Rc::new(ItemScope {
                    id,
                    value: RefCell::new(value.clone()),
                    active: Rc::new(Cell::new(true)),
                    renders: Cell::new(0),
                    template_node: state.node,
                    slot: state.slot,
                    key_path: state.key_path.clone(),
                    parent: state.parent.clone(),
                    position,
                    owner: owner.clone(),
                    bound: RefCell::new(Vec::new()),
                    repeats: RefCell::new(Vec::new()),
                    row: RefCell::new(None),
                });
                self.items.borrow_mut().insert(id, Rc::downgrade(&item));
                item
            },
            |item, value| {
                *item.value.borrow_mut() = value.clone();
                self.controls.schedule_scope(item.id, &mut app.borrow_mut());
                if let Some(row) = item.row.borrow().as_ref() {
                    app.borrow_mut().notify(row.entity_id());
                }
                for key in item.bound.borrow().iter() {
                    if let Some(bound) = self.bound.borrow().get(key) {
                        app.borrow_mut().notify(bound.entity_id());
                    }
                }
            },
            |item| self.dispose_item(&item),
        )
    }

    fn dispose_item(&self, item: &ItemScope) {
        item.active.set(false);
        self.items.borrow_mut().remove(&item.id);
        for key in item.bound.borrow().iter() {
            self.bound.borrow_mut().remove(key);
        }
        self.controls.remove_scope(item.id);
        for key in item.repeats.borrow().iter() {
            let nested = self.repeats.borrow_mut().remove(key);
            if let Some(nested) = nested {
                for entry in nested.model.borrow().rows.iter() {
                    self.dispose_item(&entry.state);
                }
            }
        }
    }
    fn render_node(
        self: &Rc<Self>,
        index: u32,
        as_entity_root: bool,
        item: Option<Rc<ItemScope>>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let template = self.template.borrow().clone();
        let Some(spec) = template.node(index) else {
            return div().into_any_element();
        };
        let scope = item.as_ref().map(|item| item.id).unwrap_or(0);
        if !spec.deps.is_empty() && !as_entity_root {
            let key = (index, scope);
            let existing = self.bound.borrow().get(&key).cloned();
            let entity = existing.unwrap_or_else(|| {
                let shared = Rc::downgrade(self);
                let entity = cx.new(|_| BoundNode {
                    shared,
                    index,
                    item: item.as_ref().map(Rc::downgrade),
                });
                self.bound.borrow_mut().insert(key, entity.clone());
                if let Some(item) = &item {
                    item.bound.borrow_mut().push(key);
                }
                entity
            });
            return entity.into_any_element();
        }
        if let Some(r) = self.renders.borrow_mut().get_mut(index as usize) {
            *r += 1;
        }
        if spec
            .when
            .is_some_and(|id| self.slots.borrow().get(id).as_bool() != Some(true))
        {
            return div().into_any_element();
        }
        let children = if let Some(slot) = spec.each {
            let key = (index, scope);
            let existing = self.repeats.borrow().get(&key).cloned();
            let state = existing.unwrap_or_else(|| {
                let state = Rc::new(Repeater {
                    model: RefCell::new(KeyedList::new(spec.key.clone())),
                    slot,
                    node: index,
                    parent: item.as_ref().map(Rc::downgrade),
                    key_path: Arc::from(spec.key.as_ref()),
                });
                let values = self.slots.borrow().get(slot).clone();
                let values = (0..values.list_len().unwrap_or(0))
                    .filter_map(|i| values.list_get(i).cloned())
                    .collect::<Arc<[Value]>>();
                self.splice_rows(&state, 0..0, &values, cx);
                self.repeats.borrow_mut().insert(key, state.clone());
                if let Some(item) = &item {
                    item.repeats.borrow_mut().push(key);
                }
                state
            });
            let rows = state.model.borrow().rows.clone();
            let count = rows.len();
            let height = guiraffe_core::Size::Xl.control_height();
            let (viewport_height, scrollable) = self.repeat_viewport.get().resolve(
                height,
                count,
                f32::from(window.viewport_size().height),
            );
            if spec.uniform_rows && scrollable {
                vec![
                    uniform_list(
                        ("data-ui-repeat", index as usize),
                        count,
                        move |range, _, cx| {
                            range
                                .filter_map(|position| rows.get(position))
                                .map(|entry| {
                                    entry
                                        .state
                                        .view(cx)
                                        .cached(
                                            gpui_kit::StyleRefinement::default()
                                                .w_full()
                                                .h(px(height)),
                                        )
                                        .into_any_element()
                                })
                                .collect()
                        },
                    )
                    .h(px(viewport_height))
                    .w_full()
                    .into_any_element(),
                ]
            } else {
                rows.iter()
                    .map(|entry| {
                        if spec.uniform_rows {
                            entry
                                .state
                                .view(cx)
                                .cached(
                                    gpui_kit::StyleRefinement::default()
                                        .w_full()
                                        .h(px(guiraffe_core::Size::Xl.control_height())),
                                )
                                .into_any_element()
                        } else {
                            entry.state.view(cx).into_any_element()
                        }
                    })
                    .collect()
            }
        } else {
            template
                .children(spec)
                .iter()
                .map(|child| self.render_node(*child, false, item.clone(), window, cx))
                .collect()
        };
        let slots = self.slots.borrow();
        let item_value = item.as_ref().map(|item| item.value.borrow());
        let view = NodeView {
            spec,
            slots: &slots,
            emit: &self.emit,
            widget_namespace: self.widget_namespace.get(),
            generation: self.generation.get(),
            index,
            item: item_value.as_deref(),
            input_emit: &self.input_emit,
            controls: &self.controls,
            scope,
            reader: &self.reader,
            #[cfg(feature = "test-support")]
            item_key: item
                .as_ref()
                .map(|item| item.value.borrow().field(&item.key_path).clone()),
            active: item.as_ref().map(|item| item.active.clone()),
        };
        spec.widget.render(&view, children, window, cx)
    }
    fn control_props(&self, index: u32, scope: u64) -> Option<crate::controls::Props> {
        let template = self.template.borrow().clone();
        let spec = template.node(index)?;
        let item = if scope == 0 {
            None
        } else {
            Some(self.items.borrow().get(&scope)?.upgrade()?)
        };
        if item.as_ref().is_some_and(|item| !item.active.get()) {
            return None;
        }
        let slots = self.slots.borrow();
        let value = item.as_ref().map(|item| item.value.borrow());
        let view = NodeView {
            spec,
            slots: &slots,
            emit: &self.emit,
            widget_namespace: self.widget_namespace.get(),
            generation: self.generation.get(),
            index,
            item: value.as_deref(),
            input_emit: &self.input_emit,
            controls: &self.controls,
            scope,
            reader: &self.reader,
            #[cfg(feature = "test-support")]
            item_key: None,
            active: None,
        };
        Some(crate::controls::Props::from_node(&view))
    }
}

/// A node with slot-bound props, rendered as its own entity for targeted invalidation.
pub struct BoundNode {
    shared: std::rc::Weak<Shared>,
    index: u32,
    item: Option<std::rc::Weak<ItemScope>>,
}

impl Render for BoundNode {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.shared
            .upgrade()
            .map(|shared| {
                shared.render_node(
                    self.index,
                    true,
                    self.item.as_ref().and_then(std::rc::Weak::upgrade),
                    window,
                    cx,
                )
            })
            .unwrap_or_else(|| div().into_any_element())
    }
}

struct ItemScope {
    id: u64,
    value: RefCell<Value>,
    active: Rc<Cell<bool>>,
    renders: Cell<u64>,
    template_node: u32,
    slot: SlotId,
    key_path: Arc<[gpui_kit::SharedString]>,
    parent: Option<std::rc::Weak<ItemScope>>,
    position: Position,
    owner: std::rc::Weak<Shared>,
    bound: RefCell<Vec<(u32, u64)>>,
    repeats: RefCell<Vec<(u32, u64)>>,
    row: RefCell<Option<Entity<ItemView>>>,
}
impl ItemScope {
    // Allocate native views only while rendering, never from a borrowed reconciler.
    // Offscreen items retain their model identity without an eagerly-created entity.
    fn view(self: &Rc<Self>, cx: &mut App) -> Entity<ItemView> {
        let existing = self.row.borrow().clone();
        existing.unwrap_or_else(|| {
            let item = Rc::downgrade(self);
            let entity = cx.new(|_| ItemView { item });
            *self.row.borrow_mut() = Some(entity.clone());
            entity
        })
    }
}
struct ItemView {
    item: std::rc::Weak<ItemScope>,
}
impl Render for ItemView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(item) = self.item.upgrade().filter(|item| item.active.get()) else {
            return div().into_any_element();
        };
        let Some(shared) = item.owner.upgrade() else {
            return div().into_any_element();
        };
        item.renders.set(item.renders.get() + 1);
        let template = shared.template.borrow().clone();
        let index = Some(item.template_node);
        let kids = index
            .and_then(|index| template.node(index))
            .map(|spec| {
                template
                    .children(spec)
                    .iter()
                    .map(|index| shared.render_node(*index, false, Some(item.clone()), window, cx))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        div().flex().flex_col().children(kids).into_any_element()
    }
}

pub struct DataUi {
    registry: Arc<Registry>,
    shared: Rc<Shared>,
}

/// A cloneable handle for pushing view data and events into a [`DataUi`] from anywhere
/// (a static parent, a background task's completion) without writing the host entity.
#[derive(Clone)]
pub struct DataUiHandle {
    shared: Rc<Shared>,
}

impl DataUiHandle {
    /// Bound uniform repeats to this height in logical pixels. `None` uses the
    /// window height. Embedded panels can supply their available content height.
    /// Returns whether the policy changed; invalid or unchanged policies are inert.
    pub fn set_repeat_max_height(
        &self,
        height: Option<f32>,
        cx: &mut App,
    ) -> Result<bool, crate::RepeatViewportError> {
        let policy = crate::RepeatViewport::new(height)?;
        if self.shared.repeat_viewport.get() == policy {
            return Ok(false);
        }
        self.shared.repeat_viewport.set(policy);
        let template = self.shared.template.borrow();
        let bound = self.shared.bound.borrow();
        for key in self.shared.repeats.borrow().keys() {
            if template.node(key.0).is_some_and(|node| node.uniform_rows)
                && let Some(entity) = bound.get(key)
            {
                cx.notify(entity.entity_id());
            }
        }
        Ok(true)
    }
    pub fn repeat_max_height(&self) -> Option<f32> {
        self.shared.repeat_viewport.get().max_height()
    }

    pub fn patch(&self, f: impl FnOnce(&Names, &mut PatchBuf), cx: &mut App) {
        self.shared.patch(f, cx);
    }
    pub fn dispatch(&self, event: impl Into<EventId>, payload: Value, cx: &mut App) {
        self.shared
            .dispatch(event.into(), payload, 0, self.shared.generation.get(), cx);
    }
    pub fn slot_value(&self, name: &str) -> Option<Value> {
        let id = self.shared.template.borrow().names().slot(name)?;
        Some(self.shared.slots.borrow().get(id).clone())
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn native_input_value(&self, node: u32, cx: &App) -> Option<Value> {
        self.shared.controls.native_value(node, 0, cx)
    }
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn native_input_value_in(&self, node: u32, scope: u64, cx: &App) -> Option<Value> {
        self.shared.controls.native_value(node, scope, cx)
    }
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn item_metrics(&self, node: u32, key: impl Into<Value>) -> Option<(u64, u64)> {
        self.item_metrics_in(node, 0, key)
    }
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn item_metrics_in(
        &self,
        node: u32,
        parent: u64,
        key: impl Into<Value>,
    ) -> Option<(u64, u64)> {
        let key = key.into();
        let template = self.shared.template.borrow();
        let path = &template.node(node)?.key;
        let states = self.shared.repeats.borrow();
        let state = states.get(&(node, parent))?.model.borrow();
        let entry = state
            .rows
            .iter()
            .find(|entry| entry.value.field(path) == &key)?;
        Some((entry.state.id, entry.state.renders.get()))
    }
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn dispatch_input_for_test(
        &self,
        event: EventId,
        slot: SlotId,
        payload: Payload,
        cx: &mut App,
    ) {
        self.shared.dispatch_input(
            InputRequest {
                event: Some(event),
                target: Some(ChangeTarget::Slot(slot)),
                scope: 0,
                payload,
                generation: self.shared.generation.get(),
            },
            cx,
        );
    }
    /// Test seam: deliver an event stamped with the *previous* document generation, as a
    /// handler captured before a hot reload would.
    #[doc(hidden)]
    pub fn dispatch_stale_for_test(&self, event: EventId, cx: &mut App) {
        let previous = self.shared.generation.get().wrapping_sub(1);
        self.shared.dispatch(event, Value::Null, 0, previous, cx);
    }
}

impl DataUi {
    pub fn new(
        doc: &Document,
        registry: Arc<Registry>,
        logic: impl Logic,
        cx: &mut App,
    ) -> Result<Entity<Self>, CompileError> {
        let template = Arc::new(Template::compile(doc, &registry)?);
        Ok(Self::from_template(template, registry, logic, cx))
    }

    /// Instantiate an already-compiled template (e.g. compiled on a background thread).
    pub fn from_template(
        template: Arc<Template>,
        registry: Arc<Registry>,
        mut logic: impl Logic,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| {
            // Built once, shared by every handler; dispatches through the shared state (never
            // by writing the host entity). The cell breaks the Rc cycle Shared ↔ emit.
            let shared_slot: Rc<RefCell<std::rc::Weak<Shared>>> = Rc::default();
            let target = shared_slot.clone();
            let emit: Emit = Rc::new(move |id, payload, scope, generation, _w, cx| {
                if let Some(shared) = target.borrow().upgrade() {
                    shared.dispatch(id, payload, scope, generation, cx);
                }
            });
            let input_target = shared_slot.clone();
            let input_emit: crate::widget::InputEmit = Rc::new(move |request, cx| {
                if let Some(shared) = input_target.borrow().upgrade() {
                    shared.dispatch_input(request, cx);
                }
            });
            let read_target = shared_slot.clone();
            let reader: crate::controls::PropsRead = Rc::new(move |node, scope| {
                read_target.borrow().upgrade()?.control_props(node, scope)
            });
            let mut slots = SlotTable::with_defaults(template.slot_defaults());
            let mut buf = PatchBuf::default();
            logic.bind(template.names(), &mut buf);
            logic.init(&mut buf);
            slots.apply(&mut buf);
            let renders = vec![0; template.len()];
            let shared = Rc::new_cyclic(|owner| Shared {
                template: RefCell::new(template),
                slots: RefCell::new(slots),
                widget_namespace: Cell::new(NEXT_WIDGET_NAMESPACE.fetch_add(1, Ordering::Relaxed)),
                generation: Cell::new(0),
                emit,
                input_emit,
                reader,
                items: RefCell::default(),
                controls: Default::default(),
                repeats: RefCell::default(),
                next_item: Cell::new(1),
                bound: RefCell::default(),
                renders: RefCell::new(renders),
                logic: RefCell::new(Box::new(logic)),
                buf: RefCell::new(buf),
                changed: RefCell::default(),
                events_handled: Cell::new(0),
                stale_events: Cell::new(0),
                host_renders: Cell::new(0),
                repeat_viewport: Cell::default(),
                owner: owner.clone(),
            });
            *shared_slot.borrow_mut() = Rc::downgrade(&shared);
            Self { registry, shared }
        })
    }

    pub fn template(&self) -> Arc<Template> {
        self.shared.template.borrow().clone()
    }

    /// Current value of slot `name` (convenience for tests and tools).
    pub fn slot_value(&self, name: &str) -> Option<Value> {
        let id = self.shared.template.borrow().names().slot(name)?;
        Some(self.shared.slots.borrow().get(id).clone())
    }

    pub fn handle(&self) -> DataUiHandle {
        DataUiHandle {
            shared: self.shared.clone(),
        }
    }

    pub fn events_handled(&self) -> u64 {
        self.shared.events_handled.get()
    }

    /// Actual invocations of the host Render implementation (not child node renders).
    pub fn host_renders(&self) -> u64 {
        self.shared.host_renders.get()
    }

    /// Events dropped because they were wired for an older document (hot reload).
    pub fn stale_events(&self) -> u64 {
        self.shared.stale_events.get()
    }

    /// How many times node `index` has rendered (render-isolation tests, devtools).
    pub fn node_renders(&self, index: u32) -> u64 {
        self.shared
            .renders
            .borrow()
            .get(index as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Hot reload: compile a new document, keep slot values by name, rebind logic.
    pub fn set_document(
        &mut self,
        doc: &Document,
        cx: &mut Context<Self>,
    ) -> Result<(), CompileError> {
        let template = Arc::new(Template::compile(doc, &self.registry)?);
        let migrated = {
            let old = self.shared.template.borrow();
            self.shared.slots.borrow().migrate(
                old.names().slot_names(),
                template.names().slot_names(),
                template.slot_defaults(),
            )
        };
        *self.shared.slots.borrow_mut() = migrated;
        *self.shared.renders.borrow_mut() = vec![0; template.len()];
        self.shared.bound.borrow_mut().clear();
        for state in self.shared.repeats.borrow().values() {
            for row in state.model.borrow().rows.iter() {
                row.state.active.set(false);
            }
        }
        self.shared.repeats.borrow_mut().clear();
        self.shared.items.borrow_mut().clear();
        self.shared.controls.clear();
        self.shared
            .widget_namespace
            .set(NEXT_WIDGET_NAMESPACE.fetch_add(1, Ordering::Relaxed));
        self.shared
            .generation
            .set(self.shared.generation.get().wrapping_add(1));
        self.shared
            .logic
            .borrow_mut()
            .bind(template.names(), &mut self.shared.buf.borrow_mut());
        *self.shared.template.borrow_mut() = template;
        self.shared
            .slots
            .borrow_mut()
            .apply(&mut self.shared.buf.borrow_mut());
        cx.notify();
        Ok(())
    }
}

impl Render for DataUi {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.shared
            .host_renders
            .set(self.shared.host_renders.get() + 1);
        let root = self.shared.template.borrow().root();
        self.shared.render_node(root, false, None, window, cx)
    }
}
