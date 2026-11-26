//! The review windows: the main list-and-diff window and the detachable compare panel.
//!
//! Written against the stable layers (Store, ActionBus, the builder API), shaped so a move
//! to store-first `ui!` is mechanical: every view reads `Selected` slices, and every
//! interaction dispatches a bus action (never mutates state directly).
use crate::{
    controls::{self, paths},
    diff::{self, Pixels},
    discover::{Golden, Source},
    review::{
        Decide, Decision, Export, LoadRequest, Loaded, LoadedGolden, Mode, Review, Row, Shared,
    },
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Hsla, ImageSource, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Pixels as Px, Render, RenderImage, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window, canvas,
    component::{button::Button, dock::BasePanel},
    div, img,
    prelude::FluentBuilder,
    px,
};
use guiraffe_commands::{
    Command, CommandId, CommandScope, Commands, Handler, Keybinding,
    ribbon::{
        CommandRibbon, RibbonButton, RibbonCategory, RibbonEntry, RibbonLayout, RibbonMenu,
        RibbonModel, RibbonState, ribbon_scope,
    },
};
use guiraffe_components::controls::{
    ButtonControl, SelectControl, SliderControl, TextInputControl,
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
use guiraffe_ui::{ControlHandle, ui};
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
    store.on_effect(cx, |crate::review::CopySummary(summary), _, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(summary));
    });
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

/// Theme colours, read where they are drawn so a theme change reaches every view.
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

fn row_view(key: u32, row: &Row, selected: bool, cx: &App) -> AnyElement {
    let p = palette(cx);
    let mark = match row.verdict.as_ref().map(|v| v.decision) {
        Some(Decision::Accept) => "✓",
        Some(Decision::Reject) => "✗",
        None => "·",
    };
    let g = &row.golden;
    let name = SharedString::from(g.name.to_string());
    let detail = format!(
        "{} · {} · {}",
        g.backend.label(),
        g.theme.label(),
        g.change.label()
    );
    // A row click is a bus action on the list control, so pointer and keyboard selection
    // take the same path.
    let select = move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
        controls::dispatch(
            paths::LIST,
            names::SELECT,
            Value::Num(f64::from(key)),
            window,
            cx,
        )
    };
    ui! {
        div {
            .id(("golden-row", key as usize))
            .test_support()
            w_full  h: px(ROW_HEIGHT)  px: px(8.)  flex items_center  gap: px(8.)
            text_color: p.text
            .when(selected, |row| row.bg(p.primary.opacity(0.18)))
            .on_click(select)
            div { w: px(14.)  {mark} }
            div { flex_1 overflow_hidden; {name} }
            div { text_xs; {detail} }
        }
    }
    .into_any_element()
}

pub const ROW_HEIGHT: f32 = 28.;

/// Left panel: the store-bound, virtualised golden list and the verdict controls.
pub struct ListPanel {
    store: Store<Review>,
    focus: FocusHandle,
    status: Selected<(usize, usize, Option<SharedString>)>,
    /// The reason field, mounted by `ui!`: keyboard-only review focuses it (Alt+T).
    reason: ControlHandle<TextInputControl>,
}

impl ListPanel {
    fn new(store: &Store<Review>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let status = store.select(cx, |s: &Review| {
            (s.decided(), s.goldens.len(), s.error.clone())
        });
        Self {
            store: store.clone(),
            focus: cx.focus_handle(),
            status,
            reason: ControlHandle::new(),
        }
    }
    /// The reason field's focus handle (keyboard-only review: Alt+T focuses it).
    pub fn reason_focus(&self, cx: &App) -> FocusHandle {
        self.reason
            .focus_handle(cx)
            .unwrap_or_else(|| self.focus.clone())
    }
}

impl Render for ListPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let (decided, total, error) = self.status.get(cx).clone();
        let status = match error {
            Some(error) => format!("{decided}/{total} reviewed · {error}"),
            None => format!("{decided}/{total} reviewed"),
        };
        let store = self.store.clone();
        let focus = self.focus.clone();
        let reason = self.reason.clone();
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
                store(store) {
                    div {
                        flex_1
                        for (key, row) in <= goldens, row_height: px(ROW_HEIGHT) {
                            div {
                                let selected <= derived(selected, move |s| *s == key);
                                {row_view(key, row, *selected, cx)}
                            }
                        }
                    }
                    div {
                        p: px(8.)  gap: px(6.)  flex flex_col  border_t_1
                        border_color: p.border
                        TextInputControl {
                            value <=> reason
                            .path(paths::REASON)
                            .placeholder("Reason (optional)")
                            handle: reason
                        }
                        div {
                            flex  gap: px(6.)
                            ButtonControl {
                                .label("Accept")
                                .path(paths::ACCEPT)
                                on_click => Decide { decision: Decision::Accept, reason: None }
                            }
                            ButtonControl {
                                .label("Reject")
                                .path(paths::REJECT)
                                on_click => Decide { decision: Decision::Reject, reason: None }
                            }
                            ButtonControl {
                                .label("Export")
                                .path(paths::EXPORT)
                                on_click => Export
                            }
                        }
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

/// The compare panel: view mode, zoom, summary, the images and pixel inspection. Detachable.
/// Everything it shows is bound in `ui!`; the panel itself never renders on a store change.
pub struct ComparePanel {
    store: Store<Review>,
    focus: FocusHandle,
    /// The image area's bounds from the last paint, for pointer-to-pixel mapping.
    bounds: Rc<Cell<Option<Bounds<Px>>>>,
}

impl ComparePanel {
    fn new(store: &Store<Review>, cx: &mut Context<Self>) -> Self {
        Self {
            store: store.clone(),
            focus: cx.focus_handle(),
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
        Some(image) => ui! {
            img(ImageSource::Render(image.clone())) {
                .id(id)
                w: px(w)  h: px(h)
            }
        }
        .into_any_element(),
        None => ui! {
            div {
                .id(id)
                w: px(w.max(80.))  h: px(h.max(40.))  border_1
                border_color: border
                "absent"
            }
        }
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

/// The summary, the pixel readout and the images for the current view.
fn compare_body(
    mode: Mode,
    zoom: f32,
    inspect: Option<(u32, u32)>,
    golden: Option<&Golden>,
    loaded: Option<&Loaded>,
    bounds: &Rc<Cell<Option<Bounds<Px>>>>,
    cx: &App,
) -> AnyElement {
    let p = palette(cx);
    let summary = summary_text(golden, loaded);
    let readout = match (inspect, loaded) {
        (Some((x, y)), Some(l)) => format!(
            "pixel ({x}, {y}) · before {} · after {}",
            pixel_text(l.before.as_ref().and_then(|p| p.get(x, y))),
            pixel_text(l.after.as_ref().and_then(|p| p.get(x, y))),
        ),
        _ => "click an image to inspect a pixel".into(),
    };
    let images: Vec<AnyElement> = match loaded {
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
                Mode::After => vec![picture(i.after.as_ref(), a, zoom, p.border, "image-after")],
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
    let at = bounds.clone();
    let paint_bounds = bounds.clone();
    let inspect_at = move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
        if let Some(b) = at.get() {
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
    let record = move |b, _: &mut Window, _: &mut App| paint_bounds.set(Some(b));
    ui! {
        div {
            flex_1 flex flex_col  gap: px(6.)
            div { .id("diff-summary") .test_support() {summary} }
            div { .id("pixel-readout") .test_support() {readout} }
            div {
                .id("compare-images")
                .test_support()
                flex_1 overflow_scroll relative
                .on_mouse_down(MouseButton::Left, inspect_at)
                div {
                    flex gap: px(8.)
                    .children(images)
                }
                canvas(record, |_, _, _, _| {}) {
                    absolute top_0 left_0 size_full
                }
            }
        }
    }
    .into_any_element()
}

/// Ask the compare panel's dock control to pop out or dock back (a bus action on the panel,
/// not a store event).
fn dock(action: &'static str) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| controls::dispatch(paths::COMPARE, action, Value::Null, window, cx)
}

impl Render for ComparePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let store = self.store.clone();
        let focus = self.focus.clone();
        let bounds = self.bounds.clone();
        let modes = Mode::ALL.map(|m| (m, m.label()));
        ui! {
            div {
                .id("compare-panel")
                .track_focus(&focus)
                .key_context("GoldenReview")
                size_full flex flex_col  gap: px(6.)  p: px(8.)  text_sm
                bg: p.body
                text_color: p.text
                store(store) {
                    div {
                        flex items_center  gap: px(8.)
                        SelectControl {
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
                        {<= derived(zoom, |z| format!("{:.0}%", z * 100.))}
                        Button("compare-pop-out") {
                            label: "Pop out"
                            .on_click(dock(names::OPEN))
                        }
                        Button("compare-dock-back") {
                            label: "Dock back"
                            .on_click(dock(names::CLOSE))
                        }
                    }
                    div {
                        let mode <= mode;
                        let zoom <= zoom;
                        let inspect <= inspect;
                        let golden <= derived((selected, goldens), |s, g| g.get(s).map(|r| r.golden.clone()));
                        let loaded <= derived((selected, goldens, loaded), |s, g, l| {
                            g.get(s).and_then(|r| l.get(&r.golden.path)).cloned()
                        });
                        flex_1 flex flex_col;
                        {compare_body(*mode, *zoom, *inspect, golden.as_ref(), loaded.as_ref(), &bounds, cx)}
                    }
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
