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
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// How often an open, unpaused panel refreshes itself (profiler numbers, highlight fades).
/// The panel never polls every frame: on upstream GPUI any refresh redraws the whole
/// window, so per-frame polling would itself become the dominant measured work.
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

/// Events shown per history page.
pub const PAGE: usize = 8;

/// Slow-motion restore: return to `previous` unless something else changed the scale
/// after we applied `applied`, in which case that newer value is kept.
pub fn restore_time_scale(previous: f32, applied: f32, current: f32) -> f32 {
    if current == applied {
        previous
    } else {
        current
    }
}

/// Next "Older events" offset: advance one page while any store still has older rows.
pub fn older_offset(offset: usize, longest: usize) -> usize {
    let next = offset.saturating_add(PAGE);
    if next < longest { next } else { offset }
}

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

impl ViewInfo {
    /// Highlight lifetime, independent of motion slow-mo and reduced-motion preferences.
    pub fn is_flashing_at(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_render) < Duration::from_millis(180)
    }
}

struct Shared {
    open: Cell<bool>,
    /// Mirrors `open`; read by `track` on every app render, so the closed path is one
    /// relaxed atomic load and no map access.
    tracking: AtomicBool,
    focus: gpui_kit::FocusHandle,
    trace: RefCell<Option<TraceGuard>>,
    collection_epoch: Cell<u64>,
    highlight: Cell<bool>,
    inspect: Cell<bool>,
    paused: Cell<bool>,
    history_offset: Cell<usize>,
    /// (previous, applied) time scale while slow motion is on.
    slow: Cell<Option<(f32, f32)>>,
    views: RefCell<BTreeMap<SharedString, ViewInfo>>,
    stores: RefCell<Vec<Rc<dyn InspectStore>>>,
}

/// Per-window handle. Changes notify the tools leaf directly and never write app entities.
///
/// ```no_run
/// use gpui_kit::{App, Entity, Render, Window};
/// use guiraffe_devtools::{Devtools, DevtoolsRoot, root};
/// fn attach<V: Render + 'static>(view: Entity<V>, window: &mut Window, cx: &mut App) -> Entity<DevtoolsRoot> {
///     let tools = Devtools::new(cx);
///     tools.focus(window, cx);
///     root(view, &tools, cx)
/// }
/// ```
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
            tracking: AtomicBool::new(false),
            focus: cx.focus_handle(),
            trace: RefCell::new(None),
            collection_epoch: Cell::new(0),
            highlight: Cell::new(true),
            inspect: Cell::new(false),
            paused: Cell::new(false),
            history_offset: Cell::new(0),
            slow: Cell::new(None),
            views: RefCell::default(),
            stores: RefCell::default(),
        });
        let panel = cx.new(|_| DevtoolsPanel {
            shared: shared.clone(),
            frames: FrameLog::new(240),
            collector: FrameTimingCollector::new(),
            collection_epoch: 0,
            ticker: None,
        });
        Self { shared, panel }
    }
    /// Mount the overlay beside content inside an existing root view. This reads no panel
    /// state, so retaining the handle in the app does not subscribe it to telemetry.
    pub fn layer(&self, content: impl IntoElement) -> AnyElement {
        let tools = self.clone();
        div()
            .size_full()
            .relative()
            .key_context("GrDevtools")
            .track_focus(&self.shared.focus)
            .on_action(move |_: &ToggleDevtools, _, cx| tools.toggle(cx))
            .child(content)
            .child(self.panel.clone())
            .into_any_element()
    }
    /// Give an initially unfocused window a keyboard target in GrDevtools. Existing child
    /// focus also works; applications should call this once when opening an unfocused root.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.shared.focus.focus(window, cx);
    }
    pub fn is_open(&self) -> bool {
        self.shared.open.get()
    }
    pub fn set_open(&self, open: bool, cx: &mut App) {
        if self.shared.set_open(open) {
            cx.notify(self.panel.entity_id());
            if open {
                // `track` records nothing while closed; one full refresh on open lets every
                // tracked view register its counters, style and bounds immediately.
                cx.refresh_windows();
            }
        }
    }
    /// Freeze sampling and highlights for repeatable visual inspection. No animation frames
    /// are requested while paused; controls still work normally.
    pub fn set_paused(&self, paused: bool, cx: &mut App) {
        if self.shared.set_paused(paused) {
            cx.notify(self.panel.entity_id());
        }
    }
    /// Enable pointer inspection without mutating any inspected application entity.
    pub fn set_inspecting(&self, inspecting: bool, cx: &mut App) {
        if self.shared.inspect.replace(inspecting) != inspecting {
            cx.notify(self.panel.entity_id());
        }
    }
    pub fn toggle(&self, cx: &mut App) {
        self.set_open(!self.is_open(), cx);
    }
    /// Connect a store. While the panel is open, each recorded event refreshes it (also
    /// when paused), coalesced to one deferred notify of the tools leaf.
    pub fn add_store(&self, store: Rc<dyn InspectStore>, cx: &mut App) {
        let shared = Rc::downgrade(&self.shared);
        let panel = self.panel.downgrade();
        let async_cx = cx.to_async();
        let pending = Rc::new(Cell::new(false));
        store.set_on_record(Some(Rc::new(move || {
            let open = shared.upgrade().is_some_and(|s| s.open.get());
            if !open || pending.replace(true) {
                return;
            }
            let panel = panel.clone();
            let pending = pending.clone();
            // Interceptors run inside the Store's flush; notify after it, not re-entrantly.
            async_cx
                .spawn(async move |cx| {
                    pending.set(false);
                    let _ = cx.update(|cx| {
                        if let Some(panel) = panel.upgrade() {
                            cx.notify(panel.entity_id());
                        }
                    });
                })
                .detach();
        })));
        self.shared.stores.borrow_mut().push(store);
        cx.notify(self.panel.entity_id());
    }
    /// Smallest tracked view beneath a screen-space pointer (not a GPUI entity read).
    pub fn inspect_at(&self, point: gpui_kit::Point<Pixels>) -> Option<ViewInfo> {
        inspect_at(&self.shared.views.borrow(), point)
    }
    /// Copy the rolling profiler samples for diagnostics. Avoid calling from an inspected
    /// view's render: that would deliberately subscribe the view to the tools entity.
    pub fn frame_samples(&self, cx: &App) -> Vec<FrameSample> {
        self.panel
            .read(cx)
            .frames
            .samples()
            .iter()
            .copied()
            .collect()
    }
    pub fn views(&self) -> Vec<ViewInfo> {
        self.shared.views.borrow().values().cloned().collect()
    }
    /// Call exactly once from a view's render, using a stable, unique id/debug selector.
    /// The wrapper captures bounds and inherited style at layout/prepaint time. No app notify.
    ///
    /// While the tools are closed this is a single atomic load: no bookkeeping, so render
    /// counters cover the time the panel is open.
    pub fn track(&self, id: impl Into<SharedString>, child: impl IntoElement) -> Tracked {
        let id = id.into();
        if !self.shared.tracking.load(Ordering::Relaxed) {
            return Tracked {
                id,
                shared: None,
                child: Some(child.into_any_element()),
            };
        }
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
            shared: Some(self.shared.clone()),
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
    shared: Option<Rc<Shared>>,
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
        if let Some(shared) = &self.shared
            && let Some(info) = shared.views.borrow_mut().get_mut(&self.id)
        {
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
        if let Some(shared) = &self.shared
            && let Some(info) = shared.views.borrow_mut().get_mut(&self.id)
        {
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
        self.tools.layer(self.content.clone())
    }
}

/// The only entity invalidated by tools controls, profiling or hover inspection.
pub struct DevtoolsPanel {
    shared: Rc<Shared>,
    frames: FrameLog,
    collector: FrameTimingCollector,
    collection_epoch: u64,
    /// Periodic refresh while open and unpaused; dropped (cancelled) otherwise.
    ticker: Option<gpui_kit::Task<()>>,
}
impl DevtoolsPanel {
    fn button(
        &self,
        id: &'static str,
        text: impl Into<SharedString>,
        cx: &Context<Self>,
        action: impl Fn(&Shared, &mut App) + 'static,
    ) -> impl IntoElement {
        let shared = self.shared.clone();
        let target = cx.entity_id();
        div().debug_selector(move || id.to_owned()).child(
            gpui_kit::component::button::Button::new(id)
                .label(text.into())
                .on_click(move |_, _, cx| {
                    action(&shared, cx);
                    cx.notify(target);
                }),
        )
    }
}
impl Render for DevtoolsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // GPUI resets its global event sequence when tracing stops. Start a new cursor
        // after acquiring a trace lease, otherwise old cursors can skip resumed samples.
        if self.collection_epoch != self.shared.collection_epoch.get() {
            self.collection_epoch = self.shared.collection_epoch.get();
            self.collector = FrameTimingCollector::new();
        }
        // Discard events from other windows; collectors have independent cursors.
        for event in self.collector.collect_unseen() {
            if let FrameEvent::Draw(frame) = event
                && frame.window_id == window.window_handle().window_id()
                && self.shared.open.get()
                && !self.shared.paused.get()
            {
                self.frames.record(FrameSample {
                    total: frame.draw_duration(),
                    layout: frame.phases.request_layout,
                    prepaint: frame.phases.prepaint,
                    paint: frame.phases.paint,
                });
            }
        }
        let active = self.shared.open.get() && !self.shared.paused.get();
        if !active {
            self.ticker = None;
        } else if self.ticker.is_none() {
            // Sample on a timer rather than every frame (see SAMPLE_INTERVAL).
            self.ticker = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(SAMPLE_INTERVAL).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        }
        if !self.shared.open.get() {
            return div().into_any_element();
        }
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let accent = t.primary_color().to_hsla();
        let shared = self.shared.clone();
        let pointer = window.mouse_position();
        let hovered = if shared.inspect.get() {
            inspect_at(&shared.views.borrow(), pointer)
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
                            && !shared.paused.get()
                            && info.is_flashing_at(now))
                    {
                        window.paint_quad(outline(info.bounds, accent, BorderStyle::Solid));
                    }
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full();
        let ms = |duration: Option<Duration>| {
            duration
                .map(|d| format!("{:.2}", d.as_secs_f64() * 1000.))
                .unwrap_or_else(|| "—".into())
        };
        let frame = self.frames.samples().back().copied();
        let mut panel = div()
            .id("devtools-panel")
            .debug_selector(|| "devtools-panel".into())
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
                    .child(self.button("devtools-close", "Close", cx, |s, _| {
                        s.set_open(false);
                    })),
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
                        if self.shared.slow.get().is_some() {
                            "Motion: 0.1×"
                        } else {
                            "Motion: 1×"
                        },
                        cx,
                        |s, cx| {
                            let mut prefs = MotionPrefs::global(cx);
                            if let Some((previous, applied)) = s.slow.take() {
                                prefs.time_scale =
                                    restore_time_scale(previous, applied, prefs.time_scale);
                            } else {
                                let applied = prefs.time_scale * 10.;
                                s.slow.set(Some((prefs.time_scale, applied)));
                                prefs.time_scale = applied;
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
            .child("Draw work (CPU), includes tools; excludes vsync / GPU presentation")
            .child(if gpui_kit::GPUI_FAST {
                "Sampled every 250 ms and on events"
            } else {
                "Upstream GPUI: each refresh redraws the window; samples include tools refreshes"
            });
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
        panel = panel.child(
            div()
                .flex()
                .gap(px(Size::Xs.spacing()))
                .child(self.button("devtools-older", "Older events", cx, |s, _| {
                    let longest = s.stores.borrow().iter().map(|st| st.len()).max();
                    s.history_offset
                        .set(older_offset(s.history_offset.get(), longest.unwrap_or(0)))
                }))
                .child(self.button("devtools-latest", "Latest events", cx, |s, _| {
                    s.history_offset.set(0)
                }))
                .child(self.button(
                    "devtools-pause",
                    if self.shared.paused.get() {
                        "Resume"
                    } else {
                        "Pause"
                    },
                    cx,
                    |s, _| {
                        s.set_paused(!s.paused.get());
                    },
                )),
        );
        for (index, store) in self.shared.stores.borrow().iter().enumerate() {
            let mut store_panel = div()
                .id(("devtools-store", index))
                .child(store.name().to_owned());
            for row in store.rows(self.shared.history_offset.get(), PAGE).iter() {
                let seq = row.seq;
                let before = store.clone();
                let after = store.clone();
                store_panel = store_panel.child(
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
                                .debug_selector(move || format!("devtools-before-{index}-{seq}"))
                                .cursor_pointer()
                                .child("Before")
                                .on_click(move |_, _, cx| {
                                    before.jump(seq, true, cx);
                                }),
                        )
                        .child(
                            div()
                                .id(("after", seq))
                                .debug_selector(move || format!("devtools-after-{index}-{seq}"))
                                .cursor_pointer()
                                .child("After")
                                .on_click(move |_, _, cx| {
                                    after.jump(seq, false, cx);
                                }),
                        ),
                );
                for change in row.changes.iter().take(8) {
                    store_panel = store_panel.child(format!(
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
            panel = panel.child(store_panel);
        }
        div()
            .absolute()
            .inset_0()
            .size_full()
            .child(drawing)
            .child(panel)
            .into_any_element()
    }
}

fn inspect_at(
    views: &BTreeMap<SharedString, ViewInfo>,
    point: gpui_kit::Point<Pixels>,
) -> Option<ViewInfo> {
    views
        .values()
        .filter(|v| v.bounds.contains(&point))
        .min_by(|a, b| {
            let area =
                |v: &ViewInfo| f32::from(v.bounds.size.width) * f32::from(v.bounds.size.height);
            area(a).total_cmp(&area(b))
        })
        .cloned()
}

// Global GPUI tracing is shared with external profilers. Leases only synchronize on
// open/pause/close, never on the app rendering or telemetry recording path.
static TRACE_USERS: std::sync::Mutex<(usize, bool)> = std::sync::Mutex::new((0, false));
struct TraceGuard;
impl TraceGuard {
    fn acquire() -> Self {
        let mut users = TRACE_USERS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if users.0 == 0 {
            users.1 = gpui_kit::profiler::trace_enabled();
            gpui_kit::profiler::set_trace_enabled(true);
        }
        users.0 += 1;
        Self
    }
}
impl Drop for TraceGuard {
    fn drop(&mut self) {
        let mut users = TRACE_USERS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        users.0 = users.0.saturating_sub(1);
        if users.0 == 0 {
            gpui_kit::profiler::set_trace_enabled(users.1);
        }
    }
}
impl Shared {
    fn sync_trace(&self) {
        if self.open.get() && !self.paused.get() {
            if self.trace.borrow().is_none() {
                *self.trace.borrow_mut() = Some(TraceGuard::acquire());
                self.collection_epoch
                    .set(self.collection_epoch.get().wrapping_add(1));
            }
        } else {
            self.trace.borrow_mut().take();
        }
    }
    fn set_open(&self, open: bool) -> bool {
        let changed = self.open.replace(open) != open;
        self.tracking.store(open, Ordering::Relaxed);
        self.sync_trace();
        changed
    }
    fn set_paused(&self, paused: bool) -> bool {
        let changed = self.paused.replace(paused) != paused;
        self.sync_trace();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_core::Scheme;
    use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

    #[test]
    fn populated_profiler_snapshots() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let mut snap = Snapshotter::new()
                .unwrap_or_else(|e| panic!("{e}"))
                .with_setup(move |cx| {
                    guiraffe_core::init(cx);
                    guiraffe_core::apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                });
            let image = snap
                .render(gpui_kit::size(px(900.), px(850.)), |_, cx| {
                    let tools = Devtools::new(cx);
                    tools.set_open(true, cx);
                    tools.set_paused(true, cx);
                    // Fixed samples validate visual formatting independently of wall-clock noise.
                    tools.panel.update(cx, |panel, _| {
                        for total in [2, 4, 6, 8, 12, 16, 24, 33, 50, 64] {
                            panel.frames.record(FrameSample {
                                total: Duration::from_millis(total),
                                layout: Duration::from_micros(500),
                                prepaint: Duration::from_micros(250),
                                paint: Duration::from_micros(750),
                            });
                        }
                    });
                    struct AppView;
                    impl Render for AppView {
                        fn render(
                            &mut self,
                            _: &mut Window,
                            cx: &mut Context<Self>,
                        ) -> impl IntoElement {
                            let surfaces = GuiraffeTheme::global(cx).surfaces();
                            div().size_full().bg(surfaces.body.to_hsla())
                        }
                    }
                    let app = cx.new(|_| AppView);
                    root(app, &tools, cx)
                })
                .unwrap_or_else(|e| panic!("{e:#}"));
            let name = if scheme == Scheme::Light {
                "profile-populated-light"
            } else {
                "profile-populated-dark"
            };
            assert_snapshot(
                name,
                &image,
                &SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
                    .update_from_env(),
            )
            .unwrap_or_else(|e| panic!("{e:#}"));
        }
    }

    #[test]
    fn slow_motion_restore_keeps_a_newer_time_scale() {
        assert_eq!(restore_time_scale(1., 10., 10.), 1., "untouched: restore");
        assert_eq!(
            restore_time_scale(1., 10., 2.),
            2.,
            "changed meanwhile: keep the newer value"
        );
    }

    #[test]
    fn older_offset_stops_at_the_longest_history() {
        assert_eq!(older_offset(0, 0), 0);
        assert_eq!(older_offset(0, 8), 0, "one page: nothing older");
        assert_eq!(older_offset(0, 9), 8);
        assert_eq!(older_offset(8, 9), 8);
    }

    #[gpui_kit::test]
    fn closed_tools_skip_tracking_and_opening_registers_views(cx: &mut gpui_kit::TestAppContext) {
        struct App2 {
            tools: Devtools,
        }
        impl Render for App2 {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                self.tools.track("tracked", div().size_full().child("x"))
            }
        }
        let mut handle = None;
        let (_, vcx) = cx.add_window_view(|_, cx| {
            guiraffe_core::init(cx);
            let tools = Devtools::new(cx);
            handle = Some(tools.clone());
            let app = cx.new(|_| App2 {
                tools: tools.clone(),
            });
            DevtoolsRoot {
                content: app.into(),
                tools,
            }
        });
        vcx.run_until_parked();
        let tools = handle.unwrap_or_else(|| panic!("tools"));
        assert!(tools.views().is_empty(), "closed: no bookkeeping");
        vcx.update(|_, cx| tools.set_open(true, cx));
        vcx.run_until_parked();
        let views = tools.views();
        assert_eq!(views.len(), 1, "opening registers tracked views");
        assert!(views[0].bounds.size.width > px(0.));
    }

    #[gpui_kit::test]
    fn paused_panel_refreshes_when_a_store_records(cx: &mut gpui_kit::TestAppContext) {
        #[derive(Debug)]
        struct Add;
        impl guiraffe_state::Event<u32> for Add {
            fn handle(self, fx: &mut guiraffe_state::Fx<'_, u32>) {
                fx.update(|n| *n += 1);
            }
        }
        struct Empty;
        impl Render for Empty {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full()
            }
        }
        let mut handle = None;
        let (_, vcx) = cx.add_window_view(|window, cx| {
            window.resize(gpui_kit::size(px(1000.), px(900.)));
            guiraffe_core::init(cx);
            let tools = Devtools::new(cx);
            let store = guiraffe_state::Store::new(0u32, cx);
            let (adapter, _) = crate::inspect_store("s", &store, 16, |n| serde_json::json!(n), cx);
            tools.add_store(adapter, cx);
            tools.set_open(true, cx);
            tools.set_paused(true, cx);
            handle = Some((tools.clone(), store));
            DevtoolsRoot {
                content: cx.new(|_| Empty).into(),
                tools,
            }
        });
        vcx.run_until_parked();
        let (_tools, store) = handle.unwrap_or_else(|| panic!("tools"));
        assert!(vcx.debug_bounds("devtools-after-0-1").is_none());
        vcx.update(|_, cx| store.dispatch(Add, cx));
        vcx.run_until_parked();
        assert!(
            vcx.debug_bounds("devtools-after-0-1").is_some(),
            "a paused panel still shows new events"
        );
    }
}
