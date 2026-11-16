//! [`DataUi`]: the host component that instantiates a document anywhere in a static UI.
//!
//! **Granular re-rendering:** every node whose props read a slot is rendered as its own small
//! entity ([`BoundNode`]). A patch notifies only the entities bound to the slots that changed;
//! the rest of the document is untouched (and, on gpui-fast, not even revisited).

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, IntoElement, Render, WeakEntity, Window, div,
};

use crate::{
    ir::Document,
    slots::{PatchBuf, SlotId, SlotTable},
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
    fn on_event(&mut self, event: EventId, payload: &Value, out: &mut PatchBuf);
}

/// A no-op logic (static documents).
pub struct NoLogic;

impl Logic for NoLogic {
    fn bind(&mut self, _: &Names, _: &mut PatchBuf) {}
    fn on_event(&mut self, _: EventId, _: &Value, _: &mut PatchBuf) {}
}

/// State shared by the host and its bound-node entities (UI thread only).
struct Shared {
    template: RefCell<Arc<Template>>,
    slots: RefCell<SlotTable>,
    generation: Cell<u32>,
    emit: Emit,
    bound: RefCell<HashMap<u32, Entity<BoundNode>>>,
    renders: RefCell<Vec<u64>>,
    logic: RefCell<Box<dyn Logic>>,
    buf: RefCell<PatchBuf>,
    changed: RefCell<Vec<SlotId>>,
    events_handled: Cell<u64>,
    stale_events: Cell<u64>,
    host_refreshes: Cell<u64>,
    host: WeakEntity<DataUi>,
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

    /// Apply buffered patches and notify only the entities bound to changed slots.
    fn flush(&self, cx: &mut App) {
        let mut changed = self.changed.borrow_mut();
        changed.clear();
        if self
            .slots
            .borrow_mut()
            .apply_tracked(&mut self.buf.borrow_mut(), &mut changed)
            == 0
        {
            return;
        }
        let template = self.template.borrow().clone();
        let mut root_dirty = false;
        let mut targets = Vec::new();
        for slot in changed.iter() {
            for node in template.slot_nodes(*slot) {
                match self.bound.borrow().get(node) {
                    Some(e) => targets.push(e.clone()),
                    // Not rendered as an entity yet (or the root itself): re-render the host.
                    None => root_dirty = true,
                }
            }
        }
        drop(changed);
        for e in targets {
            // A pure notification. `entity.update(..)` would count as a *write* on gpui-fast and
            // invalidate every view that rendered the entity (i.e. its ancestors).
            cx.notify(e.entity_id());
        }
        if root_dirty {
            self.host_refreshes.set(self.host_refreshes.get() + 1);
            let _ = self.host.update(cx, |_, cx| cx.notify());
        }
    }

    fn render_node(
        self: &Rc<Self>,
        index: u32,
        as_entity_root: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let template = self.template.borrow().clone();
        let Some(spec) = template.node(index) else {
            return div().into_any_element();
        };
        if !spec.deps.is_empty() && !as_entity_root {
            let existing = self.bound.borrow().get(&index).cloned();
            let entity = existing.unwrap_or_else(|| {
                let shared = self.clone();
                let e = cx.new(|_| BoundNode { shared, index });
                self.bound.borrow_mut().insert(index, e.clone());
                e
            });
            return entity.into_any_element();
        }
        if let Some(r) = self.renders.borrow_mut().get_mut(index as usize) {
            *r += 1;
        }
        let children: Vec<AnyElement> = template
            .children(spec)
            .iter()
            .map(|c| self.render_node(*c, false, window, cx))
            .collect();
        let slots = self.slots.borrow();
        let view = NodeView {
            spec,
            slots: &slots,
            emit: &self.emit,
            generation: self.generation.get(),
            index,
        };
        spec.widget.render(&view, children, window, cx)
    }
}

/// A node with slot-bound props, rendered as its own entity for targeted invalidation.
pub struct BoundNode {
    shared: Rc<Shared>,
    index: u32,
}

impl Render for BoundNode {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.shared.render_node(self.index, true, window, cx)
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
    pub fn dispatch(&self, event: EventId, payload: Value, cx: &mut App) {
        self.shared
            .dispatch(event, payload, self.shared.generation.get(), cx);
    }
    pub fn slot_value(&self, name: &str) -> Option<Value> {
        let id = self.shared.template.borrow().names().slot(name)?;
        Some(self.shared.slots.borrow().get(id).clone())
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
        cx.new(|cx| {
            let host: WeakEntity<Self> = cx.entity().downgrade();
            // Built once, shared by every handler; dispatches through the shared state (never
            // by writing the host entity). The cell breaks the Rc cycle Shared ↔ emit.
            let shared_slot: Rc<RefCell<std::rc::Weak<Shared>>> = Rc::default();
            let target = shared_slot.clone();
            let emit: Emit = Rc::new(move |id, payload, generation, _w, cx| {
                if let Some(shared) = target.borrow().upgrade() {
                    shared.dispatch(id, payload, generation, cx);
                }
            });
            let mut slots = SlotTable::with_defaults(template.slot_defaults());
            let mut buf = PatchBuf::default();
            logic.bind(template.names(), &mut buf);
            logic.init(&mut buf);
            slots.apply(&mut buf);
            let renders = vec![0; template.len()];
            let shared = Rc::new(Shared {
                template: RefCell::new(template),
                slots: RefCell::new(slots),
                generation: Cell::new(0),
                emit,
                bound: RefCell::default(),
                renders: RefCell::new(renders),
                logic: RefCell::new(Box::new(logic)),
                buf: RefCell::new(buf),
                changed: RefCell::default(),
                events_handled: Cell::new(0),
                stale_events: Cell::new(0),
                host_refreshes: Cell::new(0),
                host,
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
        self.shared.render_node(root, false, window, cx)
    }
}
