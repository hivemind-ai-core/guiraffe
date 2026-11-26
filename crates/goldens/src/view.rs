//! The review windows: the main list-and-diff window and the detachable compare panel.
//!
//! Written against the stable layers (Store, ActionBus, the builder API), shaped so a move
//! to store-first `ui!` is mechanical: every view reads `Selected` slices, and every
//! interaction dispatches a bus action (never mutates state directly).
use crate::{
    controls::{self, paths},
    diff::{self, Pixels},
    discover::{Golden, Source},
    review::{Decide, Decision, LoadRequest, Loaded, LoadedGolden, Mode, Review, Row, Shared},
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Hsla, ImageSource, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Pixels as Px, Render, RenderImage, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window, canvas, component::dock::BasePanel, div, img,
    prelude::FluentBuilder, px,
};
use guiraffe_commands::{
    Command, CommandId, CommandScope, Commands, Handler, Keybinding,
    ribbon::{
        CommandRibbon, RibbonButton, RibbonCategory, RibbonEntry, RibbonLayout, RibbonMenu,
        RibbonModel, RibbonState, ribbon_scope,
    },
};
use guiraffe_components::controls::{
    ButtonControl, ButtonEvent, SelectControl, SliderControl, TextInputControl, TextInputEvent,
};
use guiraffe_core::{
    GuiraffeTheme,
    protocol::{Action, names},
    widget_value::Value,
};
use guiraffe_dnd::dock::{
    DockArea, DockDragOverlay, DockPlacement, DockablePanel, Panel, PanelEvent, panel_handle,
};
use guiraffe_state::{Selected, Store};
use guiraffe_ui::ui;
use std::{cell::Cell, rc::Rc, sync::Arc};
/// GPU-ready images of a loaded golden (BGRA, built off the UI thread).
pub struct Images {
    pub before: Option<Arc<RenderImage>>,
    pub after: Option<Arc<RenderImage>>,
    pub heat: Option<Arc<RenderImage>>,
}

impl std::fmt::Debug for Images {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Images")
    }
}

fn render_image(pixels: &Pixels) -> Option<Arc<RenderImage>> {
    if pixels.width == 0 || pixels.height == 0 {
        return None;
    }
    let mut bgra = pixels.rgba.clone();
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(pixels.width, pixels.height, bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::smallvec![
        image::Frame::new(buffer)
    ])))
}

/// Read both sides of a golden, compare them and build the images. Pure apart from the
/// source's I/O; runs on a background thread.
pub fn load(source: &dyn Source, golden: &Golden) -> anyhow::Result<Loaded> {
    let decode = |bytes: Option<Vec<u8>>| bytes.map(|b| diff::decode_png(&b)).transpose();
    let before = decode(source.before(golden))?;
    let after = decode(source.after(golden))?;
    anyhow::ensure!(
        before.is_some() || after.is_some(),
        "{}: neither side could be read",
        golden.path
    );
    let (summary, heat) = diff::diff(before.as_ref(), after.as_ref());
    let images = Images {
        before: before.as_ref().and_then(render_image),
        after: after.as_ref().and_then(render_image),
        heat: render_image(&heat),
    };
    let share = |p: Pixels| Shared(Arc::new(p));
    Ok(Loaded {
        summary,
        before: before.map(share),
        after: after.map(share),
        heat: share(heat),
        images: Shared(Arc::new(images)),
    })
}

/// Run load requests on the background executor and feed results into the store.
pub fn install_loader(store: &Store<Review>, source: Arc<dyn Source>, cx: &mut App) {
    store.on_effect(cx, move |LoadRequest(goldens), store, cx| {
        let weak = store.downgrade();
        for golden in goldens {
            let source = source.clone();
            let task = cx.background_spawn(async move {
                let result = load(&*source, &golden);
                (golden.path.clone(), result)
            });
            let weak = weak.clone();
            cx.spawn(async move |cx| {
                let (path, result) = task.await;
                cx.update(|cx| {
                    if let Some(store) = weak.upgrade() {
                        match result {
                            Ok(loaded) => store.dispatch(LoadedGolden(path, loaded), cx),
                            Err(e) => {
                                store.dispatch(crate::review::Failed(e.to_string().into()), cx)
                            }
                        }
                    }
                });
            })
            .detach();
        }
    });
}

/// Theme colours a view captures for its bound rows (row bodies get no `cx`).
#[derive(Clone, Copy)]
struct Palette {
    body: Hsla,
    text: Hsla,
    border: Hsla,
    primary: Hsla,
}

fn palette(cx: &App) -> Palette {
    let theme = GuiraffeTheme::global(cx);
    let s = theme.surfaces();
    Palette {
        body: s.body.to_hsla(),
        text: s.text.to_hsla(),
        border: s.border.to_hsla(),
        primary: theme.primary_color().to_hsla(),
    }
}

/// A protocol button on the bus at `path`; activation (pointer, keyboard, shortcut,
/// Spotlight, ribbon, MCP) runs `on_activate` once.
fn button(
    path: &'static str,
    label: &'static str,
    window: &mut Window,
    cx: &mut App,
    on_activate: impl Fn(&mut App) + 'static,
) -> (Entity<ButtonControl>, Subscription) {
    let button = ButtonControl::new()
        .label(label)
        .path(path)
        .build(window, cx);
    let subscription = cx.subscribe(&button, move |_, event: &ButtonEvent, cx| {
        let ButtonEvent::Activated(_) = event;
        on_activate(cx);
    });
    (button, subscription)
}

fn row_view(key: u32, row: &Row, p: Palette) -> AnyElement {
    let mark = match row.verdict.as_ref().map(|v| v.decision) {
        Some(Decision::Accept) => "✓",
        Some(Decision::Reject) => "✗",
        None => "·",
    };
    let g = &row.golden;
    div()
        .id(("golden-row", key as usize))
        .test_support()
        .w_full()
        .h(px(ROW_HEIGHT))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_color(p.text)
        .when(row.selected, |row| row.bg(p.primary.opacity(0.18)))
        .on_click(move |_, window, cx| {
            controls::dispatch(
                paths::LIST,
                names::SELECT,
                Value::Num(f64::from(key)),
                window,
                cx,
            )
        })
        .child(div().w(px(14.)).child(mark))
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .child(SharedString::from(g.name.to_string())),
        )
        .child(div().text_xs().child(format!(
            "{} · {} · {}",
            g.backend.label(),
            g.theme.label(),
            g.change.label()
        )))
        .into_any_element()
}

pub const ROW_HEIGHT: f32 = 28.;

/// Left panel: the store-bound, virtualised golden list and the verdict controls.
pub struct ListPanel {
    store: Store<Review>,
    focus: FocusHandle,
    status: Selected<(usize, usize, Option<SharedString>)>,
    buttons: [Entity<ButtonControl>; 3],
    _activations: [Subscription; 3],
    /// Bound by hand rather than in `ui!`: a `ui!`-built control has no handle, so it could
    /// not be focused for keyboard-only review (API feedback).
    reason: Entity<TextInputControl>,
    reason_text: Selected<Option<SharedString>>,
    mirrored: SharedString,
    _typed: Subscription,
}

impl ListPanel {
    fn new(store: &Store<Review>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let status = store.select(cx, |s: &Review| {
            (s.decided(), s.goldens.len(), s.error.clone())
        });
        let decide = |decision| {
            let store = store.downgrade();
            move |cx: &mut App| {
                if let Some(store) = store.upgrade() {
                    store.dispatch_now(
                        Decide {
                            decision,
                            reason: None,
                        },
                        cx,
                    );
                }
            }
        };
        let exported = store.downgrade();
        let (accept, a) = button(
            paths::ACCEPT,
            "Accept",
            window,
            cx,
            decide(Decision::Accept),
        );
        let (reject, r) = button(
            paths::REJECT,
            "Reject",
            window,
            cx,
            decide(Decision::Reject),
        );
        let (export, e) = button(paths::EXPORT, "Export", window, cx, move |cx| {
            if let Some(store) = exported.upgrade() {
                let summary = store.state(cx).commit_summary();
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(summary));
            }
        });
        let reason = TextInputControl::new()
            .path(paths::REASON)
            .placeholder("Reason (optional)")
            .build(window, cx);
        let typed = {
            let store = store.downgrade();
            cx.subscribe(&reason, move |_, _, event: &TextInputEvent, cx| {
                let TextInputEvent::Changed(text, source) = event;
                if let Some(store) = store.upgrade() {
                    store.set_from(Review::reason, text.clone(), *source, cx);
                }
            })
        };
        let reason_text = store.select_lens(Review::reason, cx);
        Self {
            store: store.clone(),
            focus: cx.focus_handle(),
            status,
            buttons: [accept, reject, export],
            _activations: [a, r, e],
            reason,
            reason_text,
            mirrored: SharedString::default(),
            _typed: typed,
        }
    }
    /// The reason field's focus handle (keyboard-only review: Alt+T focuses it).
    pub fn reason_focus(&self, cx: &App) -> FocusHandle {
        self.reason.read(cx).input().read(cx).focus_handle(cx)
    }
}

impl Render for ListPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Store -> field, only when the store changed (a verdict clears it, MCP sets it).
        // The setter does not emit, so this never echoes back as an edit.
        let wanted = self.reason_text.get(cx).clone().unwrap_or_default();
        if wanted != self.mirrored {
            self.mirrored = wanted.clone();
            if *self.reason.read(cx).value() != wanted {
                self.reason
                    .update(cx, |field, cx| field.set_value(wanted, cx));
            }
        }
        let reason = self.reason.clone();
        let p = palette(cx);
        let (decided, total, error) = self.status.get(cx).clone();
        let status = match error {
            Some(error) => format!("{decided}/{total} reviewed · {error}"),
            None => format!("{decided}/{total} reviewed"),
        };
        let store = self.store.clone();
        let [accept, reject, export] = self.buttons.clone();
        let focus = self.focus.clone();
        ui! {
            div {
                .id("golden-list-panel")
                .track_focus(&focus)
                .key_context("GoldenReview")
                size_full flex flex_col text_sm
                bg: p.body
                text_color: p.text
                div {
                    px: px(8.)  py: px(6.)  border_b_1
                    border_color: p.border
                    {status}
                }
                div {
                    flex_1
                    store(store) {
                        for (key, row) in <= goldens, row_height: px(ROW_HEIGHT) {
                            {row_view(key, row, p)}
                        }
                    }
                }
                div {
                    p: px(8.)  gap: px(6.)  flex flex_col  border_t_1
                    border_color: p.border
                    view(reason)
                    div {
                        flex  gap: px(6.)
                        view(accept)
                        view(reject)
                        view(export)
                    }
                }
            }
        }
    }
}
impl EventEmitter<PanelEvent> for ListPanel {}
impl Focusable for ListPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for ListPanel {
    fn panel_name(&self) -> &'static str {
        "Goldens"
    }
}
impl Panel for ListPanel {}

/// What the compare panel reads: mode, zoom, inspected pixel, the golden and its load.
type CompareView = (
    Mode,
    f32,
    Option<(u32, u32)>,
    Option<Golden>,
    Option<Loaded>,
);

/// The compare panel: view mode, zoom, summary, the images and pixel inspection. Detachable.
pub struct ComparePanel {
    store: Store<Review>,
    focus: FocusHandle,
    view: Selected<CompareView>,
    /// The image area's bounds from the last paint, for pointer-to-pixel mapping.
    bounds: Rc<Cell<Option<Bounds<Px>>>>,
}

impl ComparePanel {
    fn new(store: &Store<Review>, cx: &mut Context<Self>) -> Self {
        let view = store.select(cx, |s: &Review| {
            (
                s.mode,
                s.zoom,
                s.inspect,
                s.current().map(|row| row.golden.clone()),
                s.current_loaded().cloned(),
            )
        });
        Self {
            store: store.clone(),
            focus: cx.focus_handle(),
            view,
            bounds: Rc::default(),
        }
    }
}

fn pixel_text(p: Option<[u8; 4]>) -> String {
    p.map_or("—".into(), |[r, g, b, a]| {
        format!("rgba({r}, {g}, {b}, {a})")
    })
}

fn picture(
    image: Option<&Arc<RenderImage>>,
    size: (u32, u32),
    zoom: f32,
    border: Hsla,
    id: &'static str,
) -> AnyElement {
    let (w, h) = (size.0 as f32 * zoom, size.1 as f32 * zoom);
    match image {
        Some(image) => img(ImageSource::Render(image.clone()))
            .id(id)
            .w(px(w))
            .h(px(h))
            .into_any_element(),
        None => div()
            .id(id)
            .w(px(w.max(80.)))
            .h(px(h.max(40.)))
            .border_1()
            .border_color(border)
            .child("absent")
            .into_any_element(),
    }
}

fn summary_text(golden: Option<&Golden>, loaded: Option<&Loaded>) -> String {
    match (golden, loaded) {
        (None, _) => "No changed goldens".into(),
        (Some(g), None) => format!("{} · loading…", g.path),
        (Some(g), Some(l)) => {
            let s = l.summary;
            let size = if s.size_changed() {
                format!(
                    " · size {}×{} → {}×{}",
                    s.before.0, s.before.1, s.after.0, s.after.1
                )
            } else {
                String::new()
            };
            format!(
                "{} · {} of {} pixels changed ({:.2}%) · max channel delta {}{size}",
                g.path,
                s.changed,
                s.total,
                s.ratio() * 100.,
                s.max_delta
            )
        }
    }
}

impl Render for ComparePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let (mode, zoom, inspect, golden, loaded) = self.view.get(cx).clone();
        let summary = summary_text(golden.as_ref(), loaded.as_ref());
        let readout = match (inspect, &loaded) {
            (Some((x, y)), Some(l)) => format!(
                "pixel ({x}, {y}) · before {} · after {}",
                pixel_text(l.before.as_ref().and_then(|p| p.get(x, y))),
                pixel_text(l.after.as_ref().and_then(|p| p.get(x, y))),
            ),
            _ => "click an image to inspect a pixel".into(),
        };
        let images: Vec<AnyElement> = match &loaded {
            None => Vec::new(),
            Some(l) => {
                let i = &l.images;
                let (b, a) = (l.summary.before, l.summary.after);
                let union = (b.0.max(a.0), b.1.max(a.1));
                match mode {
                    Mode::Before => vec![picture(
                        i.before.as_ref(),
                        b,
                        zoom,
                        p.border,
                        "image-before",
                    )],
                    Mode::After => {
                        vec![picture(i.after.as_ref(), a, zoom, p.border, "image-after")]
                    }
                    Mode::Diff => vec![picture(
                        i.heat.as_ref(),
                        union,
                        zoom,
                        p.border,
                        "image-diff",
                    )],
                    Mode::SideBySide => vec![
                        picture(i.before.as_ref(), b, zoom, p.border, "image-before"),
                        picture(i.after.as_ref(), a, zoom, p.border, "image-after"),
                    ],
                }
            }
        };
        let bounds = self.bounds.clone();
        let paint_bounds = self.bounds.clone();
        let inspect_at = move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            if let Some(b) = bounds.get() {
                let x = f32::from(e.position.x - b.origin.x) / zoom;
                let y = f32::from(e.position.y - b.origin.y) / zoom;
                if x >= 0. && y >= 0. {
                    let at = Value::List(Arc::from([
                        Value::Num(f64::from(x.floor())),
                        Value::Num(f64::from(y.floor())),
                    ]));
                    controls::dispatch(paths::INSPECT, names::SET_VALUE, at, window, cx);
                }
            }
        };
        let pop = |id: &'static str, label: &'static str, action: &'static str| {
            gpui_kit::component::button::Button::new(id)
                .label(label)
                .on_click(move |_, window, cx| {
                    controls::dispatch(paths::COMPARE, action, Value::Null, window, cx)
                })
        };
        let store = self.store.clone();
        let focus = self.focus.clone();
        let modes = Mode::ALL.map(|m| (m, m.label()));
        ui! {
            div {
                .id("compare-panel")
                .track_focus(&focus)
                .key_context("GoldenReview")
                size_full flex flex_col  gap: px(6.)  p: px(8.)  text_sm
                bg: p.body
                text_color: p.text
                div {
                    flex items_center  gap: px(8.)
                    store(store) {
                        SelectControl::<Mode> {
                            value <=> mode
                            .path(paths::MODE)
                            .label("View")
                            .values(modes)
                        }
                        div {
                            w: px(160.)
                            SliderControl {
                                value <=> zoom
                                .path(paths::ZOOM)
                                .label("Zoom")
                                .range(crate::review::MIN_ZOOM, crate::review::MAX_ZOOM)
                                .step(crate::review::ZOOM_STEP)
                            }
                        }
                    }
                    {format!("{:.0}%", zoom * 100.)}
                    {pop("compare-pop-out", "Pop out", names::OPEN)}
                    {pop("compare-dock-back", "Dock back", names::CLOSE)}
                }
                div { .id("diff-summary") .test_support() {summary} }
                div { .id("pixel-readout") .test_support() {readout} }
                div {
                    .id("compare-images")
                    .test_support()
                    flex_1 overflow_scroll relative
                    .on_mouse_down(MouseButton::Left, inspect_at)
                    div { flex; gap: px(8.); .children(images) }
                    {canvas(move |b, _, _| paint_bounds.set(Some(b)), |_, _, _, _| {}).absolute().top_0().left_0().size_full()}
                }
            }
        }
    }
}

impl EventEmitter<PanelEvent> for ComparePanel {}
impl Focusable for ComparePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for ComparePanel {
    fn panel_name(&self) -> &'static str {
        "Compare"
    }
}
impl Panel for ComparePanel {}

/// Keyboard-only review: every shortcut is a declarative bus binding.
pub const SHORTCUTS: &[(&str, &str, &str, &str, &str)] = &[
    // (command id, title, keybinding, control path, action)
    (
        "goldens.next",
        "Next golden",
        "down",
        paths::LIST,
        names::INCREMENT,
    ),
    (
        "goldens.previous",
        "Previous golden",
        "up",
        paths::LIST,
        names::DECREMENT,
    ),
    (
        "goldens.accept",
        "Accept golden",
        "alt+a",
        paths::ACCEPT,
        names::ACTIVATE,
    ),
    (
        "goldens.reject",
        "Reject golden",
        "alt+r",
        paths::REJECT,
        names::ACTIVATE,
    ),
    (
        "goldens.zoom-in",
        "Zoom in",
        "alt+i",
        paths::ZOOM,
        names::INCREMENT,
    ),
    (
        "goldens.zoom-out",
        "Zoom out",
        "alt+o",
        paths::ZOOM,
        names::DECREMENT,
    ),
    (
        "goldens.export",
        "Export verdicts",
        "alt+e",
        paths::EXPORT,
        names::ACTIVATE,
    ),
    (
        "goldens.pop-out",
        "Pop out compare",
        "alt+p",
        paths::COMPARE,
        names::OPEN,
    ),
    (
        "goldens.dock-back",
        "Dock back compare",
        "alt+d",
        paths::COMPARE,
        names::CLOSE,
    ),
];

pub const MODE_KEYS: [&str; 4] = ["alt+1", "alt+2", "alt+3", "alt+4"];

fn register_commands(commands: &Commands, cx: &mut App) -> anyhow::Result<()> {
    let bound = |id: &str,
                 title: &str,
                 key: &str,
                 path: &str,
                 action: Action|
     -> anyhow::Result<Command<Handler>> {
        Ok(Command::new(
            CommandId::parse(id)?,
            title,
            Handler::apply(path.to_owned(), action),
        )
        .category("Goldens")
        .keybinding(Keybinding::parse(key)?))
    };
    for (id, title, key, path, name) in SHORTCUTS {
        let action = Action {
            name: (*name).into(),
            payload: Value::Null,
        };
        commands.register_handler(bound(id, title, key, path, action)?, cx)?;
    }
    for (mode, key) in Mode::ALL.into_iter().zip(MODE_KEYS) {
        let action = Action {
            name: names::SELECT.into(),
            payload: Value::Num(mode.index() as f64),
        };
        let id = format!("goldens.view-{}", mode.slug());
        commands.register_handler(
            bound(
                &id,
                &format!("View {}", mode.label()),
                key,
                paths::MODE,
                action,
            )?,
            cx,
        )?;
    }
    Ok(())
}

/// The main window's root: the dock with both panels, the commands and the controls.
pub struct ReviewApp {
    store: Store<Review>,
    commands: Commands,
    ribbon: Option<Entity<RibbonState>>,
    _ribbon_watch: Option<Subscription>,
    dock: Entity<DockArea>,
    overlay: Entity<DockDragOverlay>,
    list: Entity<ListPanel>,
    compare: DockablePanel<ComparePanel>,
    _controls: Vec<Entity<controls::ReviewControl>>,
}

impl ReviewApp {
    pub fn new(store: Store<Review>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let controls = controls::register_custom(&store, window, cx);
        let commands = Commands::new(cx);
        if let Err(e) = register_commands(&commands, cx) {
            store.dispatch(crate::review::Failed(format!("shortcuts: {e}").into()), cx);
        }
        let (dock, overlay) = guiraffe_dnd::dock::new_dock("goldens-dock", 1, window, cx);
        let list = cx.new(|cx| ListPanel::new(&store, window, cx));
        let compare = cx.new(|cx| ComparePanel::new(&store, cx));
        // Focus is UI, not state: the one command that is a closure, not a bus binding.
        let weak_list = list.downgrade();
        let reason = CommandId::parse("goldens.reason")
            .map_err(anyhow::Error::from)
            .and_then(|id| {
                let command = Command::new(
                    id,
                    "Type a reason",
                    move |_: &guiraffe_commands::Payload, window: &mut Window, cx: &mut App| {
                        if let Some(list) = weak_list.upgrade() {
                            let focus = list.read(cx).reason_focus(cx);
                            window.focus(&focus, cx);
                        }
                    },
                )
                .category("Goldens")
                .keybinding(Keybinding::parse("alt+t")?);
                Ok(commands.register(command, cx)?)
            });
        if let Err(e) = reason {
            store.dispatch(crate::review::Failed(format!("shortcuts: {e}").into()), cx);
        }
        dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(list.clone()),
                DockPlacement::Left,
                Some(px(340.)),
                window,
                cx,
            );
            dock.set_center(
                gpui_kit::component::dock::DockLayout::tabs()
                    .panel_view(panel_handle(compare.clone()), cx),
                window,
                cx,
            );
        });
        let compare = DockablePanel::new(paths::COMPARE, dock.clone(), compare, window, cx);
        if let Err(e) = compare.restore(window, cx) {
            store.dispatch(crate::review::Failed(format!("restore: {e}").into()), cx);
        }
        let ribbon = ribbon_model(&commands, cx).map(|model| {
            let commands = commands.clone();
            cx.new(|cx| RibbonState::new(&commands, model, window, cx))
        });
        let ribbon = match ribbon {
            Ok(ribbon) => Some(ribbon),
            Err(e) => {
                store.dispatch(crate::review::Failed(format!("ribbon: {e}").into()), cx);
                None
            }
        };
        let ribbon_watch = ribbon
            .as_ref()
            .map(|ribbon| cx.observe(ribbon, |_, _, cx| cx.notify()));
        let focus = list.read(cx).focus.clone();
        window.focus(&focus, cx);
        Self {
            store,
            commands,
            ribbon,
            _ribbon_watch: ribbon_watch,
            dock,
            overlay,
            list,
            compare,
            _controls: controls,
        }
    }
    pub fn store(&self) -> &Store<Review> {
        &self.store
    }
    pub fn commands(&self) -> &Commands {
        &self.commands
    }
    pub fn compare(&self) -> &DockablePanel<ComparePanel> {
        &self.compare
    }
    pub fn list(&self) -> &Entity<ListPanel> {
        &self.list
    }
}

impl ReviewApp {
    pub fn ribbon(&self) -> Option<&Entity<RibbonState>> {
        self.ribbon.as_ref()
    }
}

/// The ribbon over the same commands as the shortcuts: Review and View categories.
fn ribbon_model(commands: &Commands, cx: &App) -> anyhow::Result<Rc<RibbonModel>> {
    let entry = |id: &str, key: &str, command: &str| {
        CommandId::parse(command).map(|command| RibbonEntry::command(id, key, command))
    };
    let category =
        |title: &str, entries: &[(&str, &str, &str)]| -> anyhow::Result<RibbonCategory> {
            entries.iter().try_fold(
                RibbonCategory::new(title),
                |category, (id, key, command)| Ok(category.entry(entry(id, key, command)?)),
            )
        };
    let layout = RibbonLayout::new("root").menu(
        RibbonMenu::new("root", "Goldens")
            .category(category(
                "Review",
                &[
                    ("accept", "a", "goldens.accept"),
                    ("reject", "r", "goldens.reject"),
                    ("next", "j", "goldens.next"),
                    ("previous", "k", "goldens.previous"),
                    ("export", "e", "goldens.export"),
                ],
            )?)
            .category(category(
                "View",
                &[
                    ("before", "1", "goldens.view-before"),
                    ("after", "2", "goldens.view-after"),
                    ("diff", "3", "goldens.view-diff"),
                    ("side", "4", "goldens.view-side-by-side"),
                    ("zoom-in", "i", "goldens.zoom-in"),
                    ("zoom-out", "o", "goldens.zoom-out"),
                    ("pop-out", "p", "goldens.pop-out"),
                    ("dock-back", "d", "goldens.dock-back"),
                ],
            )?),
    );
    let state = commands.entity().read(cx);
    Ok(Rc::new(RibbonModel::build(
        &layout,
        state.catalog(),
        &std::collections::BTreeMap::new(),
    )?))
}

impl Render for ReviewApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let content = CommandScope::new(&self.commands).flex_1().child(
            div().size_full().relative().child(self.dock.clone()).child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(self.overlay.clone()),
            ),
        );
        let Some(ribbon) = &self.ribbon else {
            return div().size_full().flex().flex_col().child(content);
        };
        ribbon_scope(ribbon)
            .size_full()
            .flex()
            .flex_col()
            .child(content)
            .children(CommandRibbon::when_open(ribbon, cx))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .border_t_1()
                    .border_color(p.border)
                    .bg(p.body)
                    .child(RibbonButton::new(ribbon)),
            )
    }
}
