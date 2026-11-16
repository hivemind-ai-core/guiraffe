//! [`DataUi`]: the host component that instantiates a document anywhere in a static UI.

use std::{rc::Rc, sync::Arc};

use gpui_kit::{AnyElement, App, AppContext, Context, Entity, IntoElement, Render, Window, div};

use crate::{
    ir::Document,
    slots::{PatchBuf, SlotTable},
    template::{CompileError, EventId, Names, Template},
    value::Value,
    widget::{Emit, NodeView, Registry},
};

/// Application logic behind a document: events in, slot patches out (spec/10).
/// Native Rust today; the same message-shaped boundary can later be a wasm component.
pub trait Logic: 'static {
    /// Called after every (re)compile so logic can cache slot/event ids by name.
    fn bind(&mut self, names: &Names);
    /// Called once, after the first bind, to populate initial view data.
    fn init(&mut self, _out: &mut PatchBuf) {}
    fn on_event(&mut self, event: EventId, payload: &Value, out: &mut PatchBuf);
}

/// A no-op logic (static documents).
pub struct NoLogic;

impl Logic for NoLogic {
    fn bind(&mut self, _: &Names) {}
    fn on_event(&mut self, _: EventId, _: &Value, _: &mut PatchBuf) {}
}

pub struct DataUi {
    registry: Arc<Registry>,
    template: Arc<Template>,
    slots: SlotTable,
    logic: Box<dyn Logic>,
    buf: PatchBuf,
    events_handled: u64,
}

impl DataUi {
    pub fn new(
        doc: &Document,
        registry: Arc<Registry>,
        mut logic: impl Logic,
        cx: &mut App,
    ) -> Result<Entity<Self>, CompileError> {
        let template = Arc::new(Template::compile(doc, &registry)?);
        let mut slots = SlotTable::with_len(template.names.slot_names().len());
        let mut buf = PatchBuf::default();
        logic.bind(&template.names);
        logic.init(&mut buf);
        slots.apply(&mut buf);
        Ok(cx.new(|_| Self {
            registry,
            template,
            slots,
            logic: Box::new(logic),
            buf,
            events_handled: 0,
        }))
    }

    pub fn template(&self) -> &Template {
        &self.template
    }

    pub fn slots(&self) -> &SlotTable {
        &self.slots
    }

    pub fn events_handled(&self) -> u64 {
        self.events_handled
    }

    /// Hot reload: compile a new document, keep slot values by name, rebind logic.
    pub fn set_document(
        &mut self,
        doc: &Document,
        cx: &mut Context<Self>,
    ) -> Result<(), CompileError> {
        let template = Arc::new(Template::compile(doc, &self.registry)?);
        self.slots = self.slots.migrate(
            self.template.names.slot_names(),
            template.names.slot_names(),
        );
        self.logic.bind(&template.names);
        self.template = template;
        cx.notify();
        Ok(())
    }

    /// Deliver an event (from a widget, or programmatically). Re-renders only if a slot changed.
    pub fn dispatch(&mut self, event: EventId, payload: Value, cx: &mut Context<Self>) {
        self.events_handled += 1;
        self.logic.on_event(event, &payload, &mut self.buf);
        if self.slots.apply(&mut self.buf) > 0 {
            cx.notify();
        }
    }

    /// Push view data from outside (e.g. a static parent or a background task).
    pub fn patch(&mut self, f: impl FnOnce(&Names, &mut PatchBuf), cx: &mut Context<Self>) {
        f(&self.template.names, &mut self.buf);
        if self.slots.apply(&mut self.buf) > 0 {
            cx.notify();
        }
    }

    fn render_node(
        &self,
        index: u32,
        emit: &Emit,
        depth: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Some(spec) = self.template.node(index) else {
            return div().into_any_element();
        };
        if depth > crate::template::MAX_DEPTH {
            return div().into_any_element();
        }
        let children: Vec<AnyElement> = self
            .template
            .children(spec)
            .map(|c| self.render_node(c, emit, depth + 1, window, cx))
            .collect();
        let view = NodeView {
            spec,
            slots: &self.slots,
            emit,
            index,
        };
        spec.widget.render(&view, children, window, cx)
    }
}

impl Render for DataUi {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let emit: Emit = Rc::new(move |id, payload, _w, cx| {
            let _ = weak.update(cx, |this, cx| this.dispatch(id, payload, cx));
        });
        let root = self.template.root;
        self.render_node(root, &emit, 0, window, cx)
    }
}
