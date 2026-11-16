use crate::{FrameLog, FrameSample, InspectStore};
use gpui_kit::profiler::{FrameEvent, FrameTimingCollector};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, BorderStyle, Bounds, Context, Element, ElementId, Entity,
    Global, GlobalElementId, InspectorElementId, KeyBinding, LayoutId, Pixels, Render,
    SharedString, Window, canvas, div, outline, prelude::*, px,
};
use guiraffe_core::{GuiraffeTheme, Size, StyleContext};
use guiraffe_motion::MotionPrefs;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

gpui_kit::actions!(guiraffe_devtools, [ToggleDevtools]);
struct Initialized;
impl Global for Initialized {}

/// Idempotently install Ctrl-Shift-I in the GrDevtools key context.
/// Applications can override the binding with their own keymap.
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    guiraffe_motion::init(cx);
    cx.bind_keys([KeyBinding::new(
        "ctrl-shift-i",
        ToggleDevtools,
        Some("GrDevtools"),
    )]);
    cx.set_global(Initialized);
}

/// Latest diagnostics for one explicitly instrumented view.
#[derive(Clone, Debug)]
pub struct ViewInfo {
    pub id: SharedString,
    pub bounds: Bounds<Pixels>,
    pub style: StyleContext,
    pub renders: u64,
    pub last_render: Instant,
}

struct Shared {
    open: Cell<bool>,
    highlight: Cell<bool>,
    inspect: Cell<bool>,
    slow_previous: Cell<Option<f32>>,
    views: RefCell<BTreeMap<SharedString, ViewInfo>>,
    stores: RefCell<Vec<Rc<dyn InspectStore>>>,
}

/// Per-window handle. Changes notify the tools leaf directly and never write app entities.
#[derive(Clone)]
pub struct Devtools {
    shared: Rc<Shared>,
    pub(crate) panel: Entity<DevtoolsPanel>,
}
impl Devtools {
    pub fn new(cx: &mut App) -> Self {
        init(cx);
        let shared = Rc::new(Shared {
            open: Cell::new(false),
            highlight: Cell::new(true),
            inspect: Cell::new(false),
            slow_previous: Cell::new(None),
            views: RefCell::default(),
            stores: RefCell::default(),
        });
        let panel = cx.new(|_| DevtoolsPanel {
            shared: shared.clone(),
            frames: FrameLog::new(240),
            collector: FrameTimingCollector::new(),
        });
        Self { shared, panel }
    }
    pub fn is_open(&self) -> bool {
        self.shared.open.get()
    }
    pub fn set_open(&self, open: bool, cx: &mut App) {
        if self.shared.open.replace(open) != open {
            cx.notify(self.panel.entity_id());
        }
    }
    pub fn toggle(&self, cx: &mut App) {
        self.set_open(!self.is_open(), cx);
    }
    pub fn add_store(&self, store: Rc<dyn InspectStore>, cx: &mut App) {
        self.shared.stores.borrow_mut().push(store);
        cx.notify(self.panel.entity_id());
    }
    pub fn views(&self) -> Vec<ViewInfo> {
        self.shared.views.borrow().values().cloned().collect()
    }
    /// Call exactly once from a view's render, using a stable, unique id/debug selector.
    /// The wrapper captures bounds and inherited style at layout/prepaint time. No app notify.
    pub fn track(&self, id: impl Into<SharedString>, child: impl IntoElement) -> Tracked {
        let id = id.into();
        let now = Instant::now();
        let mut views = self.shared.views.borrow_mut();
        let info = views.entry(id.clone()).or_insert_with(|| ViewInfo {
            id: id.clone(),
            bounds: Bounds::default(),
            style: StyleContext::current(),
            renders: 0,
            last_render: now,
        });
        info.renders = info.renders.saturating_add(1);
        info.last_render = now;
        Tracked {
            id,
            shared: self.shared.clone(),
            child: Some(child.into_any_element()),
        }
    }
    /// Remove diagnostics when a tracked view is unmounted (ids may then be reused).
    pub fn untrack(&self, id: &str) {
        self.shared
            .views
            .borrow_mut()
            .retain(|key, _| key.as_ref() != id);
    }
}

/// Transparent instrumentation element: delegates layout, prepaint and paint to its child.
pub struct Tracked {
    id: SharedString,
    shared: Rc<Shared>,
    child: Option<AnyElement>,
}
impl IntoElement for Tracked {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Tracked {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        if let Some(info) = self.shared.views.borrow_mut().get_mut(&self.id) {
            info.style = StyleContext::current();
        }
        let mut child = self
            .child
            .take()
            .unwrap_or_else(|| gpui_kit::Empty.into_any_element());
        (child.request_layout(window, cx), child)
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(info) = self.shared.views.borrow_mut().get_mut(&self.id) {
            info.bounds = bounds;
        }
        child.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
    }
}

/// Root host with an independent sibling tools leaf. It never reads the panel entity.
pub struct DevtoolsRoot {
    content: AnyView,
    tools: Devtools,
}
pub fn root<V: Render + 'static>(
    content: Entity<V>,
    tools: &Devtools,
    cx: &mut App,
) -> Entity<DevtoolsRoot> {
    cx.new(|_| DevtoolsRoot {
        content: content.into(),
        tools: tools.clone(),
    })
}
impl Render for DevtoolsRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tools = self.tools.clone();
        div()
            .size_full()
            .relative()
            .key_context("GrDevtools")
            .on_action(move |_: &ToggleDevtools, _, cx| tools.toggle(cx))
            .child(self.content.clone())
            .child(self.tools.panel.clone())
    }
}

/// The only entity invalidated by tools controls, profiling or hover inspection.
pub struct DevtoolsPanel {
    shared: Rc<Shared>,
    frames: FrameLog,
    collector: FrameTimingCollector,
}
impl DevtoolsPanel {
    fn button(
        &self,
        id: &'static str,
        text: impl Into<SharedString>,
        cx: &Context<Self>,
        action: impl Fn(&Shared, &mut App) + 'static,
    ) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .id(id)
            .cursor_pointer()
            .px(px(Size::Xs.spacing()))
            .py(px(Size::Xs.spacing() / 2.))
            .rounded(px(Size::Xs.radius()))
            .bg(s.track.to_hsla())
            .child(text.into())
            .on_click(cx.listener(move |panel, _, _, cx| {
                action(&panel.shared, cx);
                cx.notify();
            }))
    }
}
impl Render for DevtoolsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Discard events from other windows; collectors have independent cursors.
        for event in self.collector.collect_unseen() {
            if let FrameEvent::Draw(frame) = event
                && frame.window_id == window.window_id()
                && self.shared.open.get()
            {
                self.frames.record(FrameSample {
                    total: frame.draw_duration(),
                    layout: frame.phases.request_layout,
                    prepaint: frame.phases.prepaint,
                    paint: frame.phases.paint,
                });
            }
        }
        if !self.shared.open.get() {
            return div().into_any_element();
        }
        // Pure notify: no entity write and no application/global telemetry dependency.
        window.request_animation_frame();
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let accent = t.primary_color().to_hsla();
        let shared = self.shared.clone();
        let pointer = window.mouse_position();
        let hovered = if shared.inspect.get() {
            shared
                .views
                .borrow()
                .values()
                .filter(|v| v.bounds.contains(&pointer))
                .min_by(|a, b| {
                    (a.bounds.size.width * a.bounds.size.height)
                        .partial_cmp(&(b.bounds.size.width * b.bounds.size.height))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned()
        } else {
            None
        };
        let hovered_id = hovered.as_ref().map(|v| v.id.clone());
        let drawing = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let now = Instant::now();
                for info in shared.views.borrow().values() {
                    let selected = hovered_id.as_ref() == Some(&info.id);
                    if selected
                        || (shared.highlight.get()
                            && now.saturating_duration_since(info.last_render)
                                < Duration::from_millis(180))
                    {
                        window.paint_quad(outline(info.bounds, accent, BorderStyle::Solid));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        let ms = |duration: Option<Duration>| {
            duration
                .map(|d| format!("{:.2}", d.as_secs_f64() * 1000.))
                .unwrap_or_else(|| "—".into())
        };
        let frame = self.frames.samples().back().copied();
        let mut panel = div()
            .id("devtools-panel")
            .absolute()
            .top(px(Size::Md.spacing()))
            .right(px(Size::Md.spacing()))
            .w(px(Size::Xl.control_height() * 9.))
            .max_h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(Size::Xs.spacing()))
            .p(px(Size::Sm.spacing()))
            .bg(s.raised.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(Size::Xs.font_size()))
            .border_1()
            .border_color(s.border.to_hsla())
            .rounded(px(t.default_radius.radius()))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child("GUIraffe Devtools · Ctrl-Shift-I")
                    .child(self.button("devtools-close", "Close", cx, |s, _| s.open.set(false))),
            )
            .child(
                div()
                    .flex()
                    .gap(px(Size::Xs.spacing()))
                    .child(self.button(
                        "devtools-highlight",
                        if self.shared.highlight.get() {
                            "Highlights: on"
                        } else {
                            "Highlights: off"
                        },
                        cx,
                        |s, _| s.highlight.set(!s.highlight.get()),
                    ))
                    .child(self.button(
                        "devtools-inspect",
                        if self.shared.inspect.get() {
                            "Inspect: on"
                        } else {
                            "Inspect: off"
                        },
                        cx,
                        |s, _| s.inspect.set(!s.inspect.get()),
                    ))
                    .child(self.button(
                        "devtools-slow",
                        if self.shared.slow_previous.get().is_some() {
                            "Motion: 0.1×"
                        } else {
                            "Motion: 1×"
                        },
                        cx,
                        |s, cx| {
                            let mut prefs = MotionPrefs::global(cx);
                            if let Some(previous) = s.slow_previous.take() {
                                prefs.time_scale = previous;
                            } else {
                                s.slow_previous.set(Some(prefs.time_scale));
                                prefs.time_scale *= 10.;
                            }
                            // Preference changes intentionally affect animations; telemetry never does.
                            MotionPrefs::set(prefs, cx);
                        },
                    )),
            )
            .child(format!(
                "Frame profiler · {} draw samples",
                self.frames.samples().len()
            ))
            .child(format!(
                "p50 {} ms    p99 {} ms",
                ms(self.frames.percentile(50)),
                ms(self.frames.percentile(99))
            ))
            .child(format!(
                "Layout {} · Prepaint {} · Paint {} ms",
                ms(frame.map(|f| f.layout)),
                ms(frame.map(|f| f.prepaint)),
                ms(frame.map(|f| f.paint))
            ))
            .child("Draw work (CPU), includes tools; excludes vsync / GPU presentation");
        for (label, count) in ["≤4", "≤8", "≤16", "≤33", "≤50", ">50"]
            .into_iter()
            .zip(self.frames.histogram())
        {
            panel = panel.child(
                div()
                    .flex()
                    .gap(px(Size::Xs.spacing()))
                    .child(format!("{label} ms  {count}"))
                    .child(
                        div()
                            .h(px(Size::Xs.spacing()))
                            .w(px((count as f32
                                / self.frames.samples().len().max(1) as f32)
                                * Size::Xl.control_height()
                                * 4.))
                            .bg(accent),
                    ),
            );
        }
        panel = panel.child("Inspector · instrumented views");
        if let Some(info) = hovered {
            panel = panel
                .child(format!("{} · {:?}", info.id, info.bounds))
                .child(format!(
                    "Density {:?} · Size {:?}",
                    info.style.density, info.style.control_size
                ))
                .child(format!(
                    "Surface {:?} · Emphasis {:?}",
                    info.style.surface, info.style.emphasis
                ));
        } else {
            panel = panel.child("Enable Inspect and hover a tracked view");
        }
        // Diagnostics are bounded by explicit registration; cap displayed view rows.
        for info in self.shared.views.borrow().values().take(12) {
            panel = panel.child(format!("{} · {} renders", info.id, info.renders));
        }
        panel = panel.child("State time travel · click Before / After to jump");
        if self.shared.stores.borrow().is_empty() {
            panel = panel.child("No Stores connected (inspect_store)");
        }
        for store in self.shared.stores.borrow().iter() {
            panel = panel.child(store.name().to_owned());
            for row in store.rows().into_iter().rev().take(8) {
                let seq = row.seq;
                let before = store.clone();
                let after = store.clone();
                panel = panel.child(
                    div()
                        .child(format!(
                            "#{} {}{}",
                            seq,
                            row.description,
                            if row.changed { "" } else { " (no change)" }
                        ))
                        .child(
                            div()
                                .id(("before", seq))
                                .cursor_pointer()
                                .child("Before")
                                .on_click(move |_, _, cx| {
                                    before.jump(seq, true, cx);
                                }),
                        )
                        .child(
                            div()
                                .id(("after", seq))
                                .cursor_pointer()
                                .child("After")
                                .on_click(move |_, _, cx| {
                                    after.jump(seq, false, cx);
                                }),
                        ),
                );
                for change in row.changes.iter().take(8) {
                    panel = panel.child(format!(
                        "{}: {} → {}",
                        change.path,
                        change
                            .before
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "∅".into()),
                        change
                            .after
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "∅".into())
                    ));
                }
            }
        }
        div()
            .absolute()
            .size_full()
            .child(drawing)
            .child(panel)
            .into_any_element()
    }
}
