//! [`DataUi`]: the host component that instantiates a document anywhere in a static UI.
//!
//! **Granular re-rendering:** every node whose props read a slot is rendered as its own small
//! entity ([`BoundNode`]). A patch notifies only the entities bound to the slots that changed;
//! the rest of the document is untouched (and, on gpui-fast, not even revisited).

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::Arc,
};

use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Window, div, px, uniform_list,
};

use crate::{
    Payload,
    ir::Document,
    repeat::KeyedList,
    slots::{Patch, PatchBuf, SlotId, SlotTable},
    template::{CompileError, EventId, Names, Template},
    value::Value,
    widget::{Emit, NodeView, Registry},
};

/// Application logic behind a document: events in, slot patches out (spec/10).
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
type RepeatState = Rc<RefCell<KeyedList<Rc<ItemScope>>>>;
struct Shared {
    template: RefCell<Arc<Template>>,
    slots: RefCell<SlotTable>,
    generation: Cell<u32>,
    emit: Emit,
    bound: RefCell<BTreeMap<(u32, u64), Entity<BoundNode>>>,
    repeats: RefCell<HashMap<(u32, u64), RepeatState>>,
    next_item: Cell<u64>,
    input_emit: crate::widget::InputEmit,
    controls: crate::controls::ControlCache,

    renders: RefCell<Vec<u64>>,
    logic: RefCell<Box<dyn Logic>>,
    buf: RefCell<PatchBuf>,
    changed: RefCell<Vec<SlotId>>,
    events_handled: Cell<u64>,
    stale_events: Cell<u64>,
    host_refreshes: Cell<u64>,
    owner: std::rc::Weak<Shared>,
}

impl Shared {
    /// Event path. Deliberately does **not** update the host entity: on gpui-fast an entity
    /// write invalidates its whole retained subtree, so only bound nodes are notified.
    fn dispatch(&self, event: EventId, payload: Value, generation: u32, cx: &mut App) {
        if generation != self.generation.get() {
            // Wired for an older document: its EventId may mean something else now.
            self.stale_events.set(self.stale_events.get() + 1);
            return;
        }
        if self.template.borrow().names().event_name(event).is_none() {
            return;
        }
        self.events_handled.set(self.events_handled.get() + 1);
        self.logic
            .borrow_mut()
            .on_event(event, &payload, &mut self.buf.borrow_mut());
        self.flush(cx);
    }

    fn patch(&self, f: impl FnOnce(&Names, &mut PatchBuf), cx: &mut App) {
        let template = self.template.borrow().clone();
        f(template.names(), &mut self.buf.borrow_mut());
        self.flush(cx);
    }

    fn dispatch_input(
        &self,
        event: Option<EventId>,
        slot: Option<SlotId>,
        payload: Payload,
        generation: u32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let _ = window;
        if generation != self.generation.get() {
            self.stale_events.set(self.stale_events.get() + 1);
            return;
        }
        self.buf.borrow_mut().take_veto();
        let previous = slot.map(|id| self.slots.borrow().get(id).clone());
        if let Some(id) = slot {
            self.buf.borrow_mut().set(id, payload.value());
            self.flush(cx);
        }
        if let Some(event) = event {
            self.events_handled.set(self.events_handled.get() + 1);
            self.logic
                .borrow_mut()
                .on_payload(event, &payload, &mut self.buf.borrow_mut());
        }
        if self.buf.borrow_mut().take_veto()
            && let (Some(id), Some(previous)) = (slot, previous)
        {
            self.buf.borrow_mut().set(id, previous);
        }
        self.flush(cx);
    }

    /// Apply patches, incrementally reconcile repeat ranges, then notify leaves purely.
    fn flush(&self, cx: &mut App) {
        let mut changed = self.changed.borrow_mut();
        changed.clear();
        let mut edits = Vec::new();
        if self.slots.borrow_mut().apply_detailed(
            &mut self.buf.borrow_mut(),
            &mut changed,
            &mut edits,
        ) == 0
        {
            return;
        }
        let template = self.template.borrow().clone();
        for edit in edits {
            let slot = match &edit {
                Patch::Set(id, _) | Patch::Splice(id, _, _) => *id,
            };
            let targets: Vec<_> = self
                .repeats
                .borrow()
                .iter()
                .filter_map(|((index, _), state)| {
                    template
                        .node(*index)
                        .filter(|spec| spec.each == Some(slot))
                        .map(|_| state.clone())
                })
                .collect();
            for state in targets {
                match &edit {
                    Patch::Set(_, value) => {
                        let values: Arc<[Value]> = (0..value.list_len().unwrap_or(0))
                            .filter_map(|i| value.list_get(i).cloned())
                            .collect();
                        let len = state.borrow().rows.len();
                        self.splice_rows(&state, 0..len, &values, cx);
                    }
                    Patch::Splice(_, range, values) => {
                        self.splice_rows(&state, range.clone(), values, cx)
                    }
                }
            }
        }
        let mut targets = std::collections::HashSet::new();
        for slot in changed.iter() {
            for index in template.slot_nodes(*slot) {
                for (_, entity) in self.bound.borrow().range((*index, 0)..=(*index, u64::MAX)) {
                    targets.insert(entity.entity_id());
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
    ) {
        // Stored rows carry weak host pointers; creating or dropping a row never writes DataUi.
        let owner = self.owner.clone();
        let app = RefCell::new(cx);
        state.borrow_mut().splice(
            range,
            values,
            |value| {
                let id = self.next_item.get();
                self.next_item.set(id + 1);
                Rc::new_cyclic(|weak| ItemScope {
                    id,
                    value: RefCell::new(value.clone()),
                    active: Rc::new(Cell::new(true)),
                    renders: Cell::new(0),
                    template_node: Cell::new(0),
                    owner: owner.clone(),
                    bound: RefCell::new(Vec::new()),
                    repeats: RefCell::new(Vec::new()),
                    row: app.borrow_mut().new(|_| ItemView { item: weak.clone() }),
                })
            },
            |item, value| {
                *item.value.borrow_mut() = value.clone();
                app.borrow_mut().notify(item.row.entity_id());
                for key in item.bound.borrow().iter() {
                    if let Some(bound) = self.bound.borrow().get(key) {
                        app.borrow_mut().notify(bound.entity_id());
                    }
                }
            },
            |item| self.dispose_item(&item),
        );
    }

    fn dispose_item(&self, item: &ItemScope) {
        item.active.set(false);
        for key in item.bound.borrow().iter() {
            self.bound.borrow_mut().remove(key);
        }
        self.controls.remove_scope(item.id);
        for key in item.repeats.borrow().iter() {
            let nested = self.repeats.borrow_mut().remove(key);
            if let Some(nested) = nested {
                for entry in nested.borrow().rows.iter() {
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
                let state = Rc::new(RefCell::new(KeyedList::new(spec.key.clone())));
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
            let rows = state.borrow().rows.clone();
            let count = rows.len();
            // Equal-height templates are virtualized; each item remains its own view.
            vec![
                uniform_list(
                    ("data-ui-repeat", index as usize),
                    count,
                    move |range, _, _| {
                        range
                            .filter_map(|position| rows.get(position))
                            .map(|entry| {
                                let row = entry.state.row.clone();
                                entry.state.template_node.set(index);
                                row.cached(
                                    gpui_kit::StyleRefinement::default()
                                        .w_full()
                                        .h(px(guiraffe_core::Size::Md.control_height())),
                                )
                                .into_any_element()
                            })
                            .collect()
                    },
                )
                .h(px(guiraffe_core::Size::Md.control_height() * 8.))
                .w_full()
                .into_any_element(),
            ]
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
            generation: self.generation.get(),
            index,
            item: item_value.as_deref(),
            input_emit: &self.input_emit,
            controls: &self.controls,
            scope,
            active: item.as_ref().map(|item| item.active.clone()),
        };
        spec.widget.render(&view, children, window, cx)
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
    template_node: Cell<u32>,
    owner: std::rc::Weak<Shared>,
    bound: RefCell<Vec<(u32, u64)>>,
    repeats: RefCell<Vec<(u32, u64)>>,
    row: Entity<ItemView>,
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
        let index = Some(item.template_node.get());
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
    pub fn patch(&self, f: impl FnOnce(&Names, &mut PatchBuf), cx: &mut App) {
        self.shared.patch(f, cx);
    }
    pub fn dispatch(&self, event: impl Into<EventId>, payload: Value, cx: &mut App) {
        self.shared
            .dispatch(event.into(), payload, self.shared.generation.get(), cx);
    }
    pub fn slot_value(&self, name: &str) -> Option<Value> {
        let id = self.shared.template.borrow().names().slot(name)?;
        Some(self.shared.slots.borrow().get(id).clone())
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn native_input_value(&self, node: u32, cx: &App) -> Option<Value> {
        self.shared.controls.native_value(node, cx)
    }
    /// Render counts and entity identities for keyed-repeat isolation tests/devtools.
    pub fn item_metrics(&self, node: u32, key: &str) -> Option<(u64, u64)> {
        let states = self.shared.repeats.borrow();
        let state = states.get(&(node, 0))?.borrow();
        let entry = state
            .rows
            .iter()
            .find(|entry| entry.value.field(&["id".into()]).as_str() == Some(key))?;
        Some((entry.state.id, entry.state.renders.get()))
    }
    /// Test seam: deliver an event stamped with the *previous* document generation, as a
    /// handler captured before a hot reload would.
    #[doc(hidden)]
    pub fn dispatch_stale_for_test(&self, event: EventId, cx: &mut App) {
        let previous = self.shared.generation.get().wrapping_sub(1);
        self.shared.dispatch(event, Value::Null, previous, cx);
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
        cx.new(|_cx| {
            // Built once, shared by every handler; dispatches through the shared state (never
            // by writing the host entity). The cell breaks the Rc cycle Shared ↔ emit.
            let shared_slot: Rc<RefCell<std::rc::Weak<Shared>>> = Rc::default();
            let target = shared_slot.clone();
            let emit: Emit = Rc::new(move |id, payload, generation, _w, cx| {
                if let Some(shared) = target.borrow().upgrade() {
                    shared.dispatch(id, payload, generation, cx);
                }
            });
            let input_target = shared_slot.clone();
            let input_emit: crate::widget::InputEmit =
                Rc::new(move |event, slot, payload, generation, window, cx| {
                    if let Some(shared) = input_target.borrow().upgrade() {
                        shared.dispatch_input(event, slot, payload, generation, window, cx);
                    }
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
                generation: Cell::new(0),
                emit,
                input_emit,
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
                host_refreshes: Cell::new(0),
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

    /// Times a patch had to re-render the whole document (a bound node without an entity).
    pub fn host_refreshes(&self) -> u64 {
        self.shared.host_refreshes.get()
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
            for row in state.borrow().rows.iter() {
                row.state.active.set(false);
            }
        }
        self.shared.repeats.borrow_mut().clear();
        self.shared.controls.clear();
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
        let root = self.shared.template.borrow().root();
        self.shared.render_node(root, false, None, window, cx)
    }
}
