//! The review windows: the main list-and-diff window and the detachable compare panel.
//!
//! Written against the stable layers (Store, ActionBus, the builder API), shaped so a move
//! to store-first `ui!` is mechanical: every view reads `Selected` slices, and every
//! interaction dispatches a bus action (never mutates state directly).
use crate::{
    controls::{self, paths},
    diff::{self, Pixels},
    discover::{Golden, Source},
    review::{Decision, LoadRequest, Loaded, LoadedGolden, Mode, Review, Shared},
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    ImageSource, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Pixels as Px, Render, RenderImage, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window, canvas,
    component::{
        button::{Button, ButtonVariants},
        dock::BasePanel,
        input::{Input, InputEvent, InputState},
    },
    div, img,
    prelude::FluentBuilder,
    px, uniform_list,
};
use guiraffe_commands::{Command, CommandId, CommandScope, Commands, Handler, Keybinding};
use guiraffe_core::{
    GuiraffeTheme,
    protocol::{Action, Source as Bus, names},
    widget_value::Value,
};
use guiraffe_dnd::dock::{
    DockArea, DockDragOverlay, DockPlacement, DockablePanel, Panel, PanelEvent, panel_handle,
};
use guiraffe_state::{Selected, Store};
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

fn pointer(
    path: &'static str,
    name: &'static str,
    payload: Value,
) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| controls::dispatch(path, name, payload.clone(), Bus::Pointer, window, cx)
}

fn colors(
    cx: &App,
) -> (
    gpui_kit::Hsla,
    gpui_kit::Hsla,
    gpui_kit::Hsla,
    gpui_kit::Hsla,
) {
    let theme = GuiraffeTheme::global(cx);
    let s = theme.surfaces();
    (
        s.body.to_hsla(),
        s.text.to_hsla(),
        s.border.to_hsla(),
        theme.primary_color().to_hsla(),
    )
}

type Verdicts = Arc<std::collections::BTreeMap<Arc<str>, crate::review::Verdict>>;

/// Left panel: the virtualised golden list and the verdict controls.
pub struct ListPanel {
    focus: FocusHandle,
    rows: Selected<(Arc<[Golden]>, usize, Verdicts)>,
    status: Selected<(usize, usize, Option<Arc<str>>)>,
    reason_text: Selected<Arc<str>>,
    /// The store value last copied into the field. Mirror only on store changes, or the
    /// mirror would fight typing (keystrokes land before their store update does).
    mirrored: Arc<str>,
    reason: Entity<InputState>,
    _typed: Subscription,
}

impl ListPanel {
    fn new(store: &Store<Review>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows = store.select(cx, |s: &Review| {
            (s.goldens.clone(), s.selected, s.verdicts.clone())
        });
        let status = store.select(cx, |s: &Review| {
            (s.decided(), s.goldens.len(), s.error.clone())
        });
        let reason_text = store.select(cx, |s: &Review| s.reason.clone());
        let reason = cx.new(|cx| InputState::new(window, cx).placeholder("Reason (optional)"));
        let typed = cx.subscribe_in(
            &reason,
            window,
            |_, input: &Entity<InputState>, e: &InputEvent, window, cx| {
                if let InputEvent::Change = e {
                    let text = input.read(cx).value();
                    controls::dispatch(
                        paths::REASON,
                        names::SET_VALUE,
                        Value::Str(text),
                        Bus::Keyboard,
                        window,
                        cx,
                    );
                }
            },
        );
        Self {
            focus: cx.focus_handle(),
            rows,
            status,
            reason_text,
            mirrored: "".into(),
            reason,
            _typed: typed,
        }
    }
    pub fn reason_focus(&self, cx: &App) -> FocusHandle {
        self.reason.read(cx).focus_handle(cx)
    }
}

pub const ROW_HEIGHT: f32 = 28.;

impl Render for ListPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Mirror the store's reason into the field (set over the bus, or cleared by a verdict).
        // Only on store changes: mirroring every frame would fight typing whenever a
        // keystroke lands before its store update does.
        let wanted = self.reason_text.get(cx).clone();
        if wanted != self.mirrored {
            self.mirrored = wanted.clone();
            if *self.reason.read(cx).value() != *wanted {
                self.reason.update(cx, |input, cx| {
                    input.set_value(wanted.to_string(), window, cx)
                });
            }
        }
        let (body, text, border, primary) = colors(cx);
        let (goldens, selected, verdicts) = self.rows.get(cx).clone();
        let (decided, total, error) = self.status.get(cx).clone();
        let count = goldens.len();
        let list = uniform_list("golden-rows", count, move |range, _, _| {
            range
                .filter_map(|i| goldens.get(i).map(|g| (i, g)))
                .map(|(i, golden)| {
                    let mark = match verdicts.get(&golden.path).map(|v| v.decision) {
                        Some(Decision::Accept) => "✓",
                        Some(Decision::Reject) => "✗",
                        None => "·",
                    };
                    div()
                        .id(("golden-row", i))
                        .test_support()
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .when(i == selected, |row| row.bg(primary.opacity(0.18)))
                        .on_click(pointer(paths::LIST, names::SELECT, Value::Num(i as f64)))
                        .child(div().w(px(14.)).child(mark))
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(SharedString::from(golden.name.to_string())),
                        )
                        .child(div().text_xs().child(format!(
                            "{} · {} · {}",
                            golden.backend.label(),
                            golden.theme.label(),
                            golden.change.label()
                        )))
                        .into_any_element()
                })
                .collect::<Vec<AnyElement>>()
        })
        .flex_1();
        div()
            .id("golden-list-panel")
            .track_focus(&self.focus)
            .key_context("GoldenReview")
            .size_full()
            .flex()
            .flex_col()
            .bg(body)
            .text_color(text)
            .text_sm()
            .child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(border)
                    .child(match &error {
                        Some(error) => format!("{decided}/{total} reviewed · {error}"),
                        None => format!("{decided}/{total} reviewed"),
                    }),
            )
            .child(list)
            .child(
                div()
                    .p(px(8.))
                    .gap(px(6.))
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(border)
                    .child(Input::new(&self.reason))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(
                                Button::new("golden-accept")
                                    .primary()
                                    .label("Accept")
                                    .on_click(pointer(paths::ACCEPT, names::ACTIVATE, Value::Null)),
                            )
                            .child(
                                Button::new("golden-reject")
                                    .danger()
                                    .label("Reject")
                                    .on_click(pointer(paths::REJECT, names::ACTIVATE, Value::Null)),
                            )
                            .child(
                                Button::new("golden-export")
                                    .label("Export")
                                    .on_click(pointer(paths::EXPORT, names::ACTIVATE, Value::Null)),
                            ),
                    ),
            )
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

/// The compare panel: mode, zoom, summary, the images and pixel inspection. Detachable.
pub struct ComparePanel {
    focus: FocusHandle,
    view: Selected<(crate::review::ViewSettings, Option<Golden>, Option<Loaded>)>,
    /// The image area's bounds from the last paint, for pointer-to-pixel mapping.
    bounds: Rc<Cell<Option<Bounds<Px>>>>,
}

impl ComparePanel {
    fn new(store: &Store<Review>, cx: &mut Context<Self>) -> Self {
        let view = store.select(cx, |s: &Review| {
            (
                s.view.clone(),
                s.current().cloned(),
                s.current_loaded().cloned(),
            )
        });
        Self {
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

impl Render for ComparePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (body, text, border, _) = colors(cx);
        let (view, golden, loaded) = self.view.get(cx).clone();
        let zoom = view.zoom;
        let toolbar = div()
            .flex()
            .gap(px(4.))
            .items_center()
            .children(Mode::ALL.map(|mode| {
                Button::new(SharedString::from(format!("mode-{}", mode.wire())))
                    .label(mode.label())
                    .when(mode == view.mode, |b| b.primary())
                    .on_click(pointer(
                        paths::MODE,
                        names::SET_VALUE,
                        Value::Str(mode.wire().into()),
                    ))
            }))
            .child(div().w(px(12.)))
            .child(Button::new("zoom-out").label("−").on_click(pointer(
                paths::ZOOM,
                names::DECREMENT,
                Value::Null,
            )))
            .child(div().child(format!("{:.0}%", zoom * 100.)))
            .child(Button::new("zoom-in").label("+").on_click(pointer(
                paths::ZOOM,
                names::INCREMENT,
                Value::Null,
            )))
            .child(div().w(px(12.)))
            .child(
                Button::new("compare-pop-out")
                    .label("Pop out")
                    .on_click(pointer(paths::COMPARE, names::OPEN, Value::Null)),
            )
            .child(
                Button::new("compare-dock-back")
                    .label("Dock back")
                    .on_click(pointer(paths::COMPARE, names::CLOSE, Value::Null)),
            );
        let summary = match (&golden, &loaded) {
            (None, _) => "No changed goldens".to_string(),
            (Some(g), None) => format!("{} · loading…", g.path),
            (Some(g), Some(l)) => {
                let s = l.summary;
                format!(
                    "{} · {} of {} pixels changed ({:.2}%) · max channel delta {}{}",
                    g.path,
                    s.changed,
                    s.total,
                    s.ratio() * 100.,
                    s.max_delta,
                    if s.size_changed() {
                        format!(
                            " · size {}×{} → {}×{}",
                            s.before.0, s.before.1, s.after.0, s.after.1
                        )
                    } else {
                        String::new()
                    }
                )
            }
        };
        let inspect = match (view.inspect, &loaded) {
            (Some((x, y)), Some(l)) => format!(
                "pixel ({x}, {y}) · before {} · after {}",
                pixel_text(l.before.as_ref().and_then(|p| p.get(x, y))),
                pixel_text(l.after.as_ref().and_then(|p| p.get(x, y))),
            ),
            _ => "click an image to inspect a pixel".into(),
        };
        let picture =
            |image: Option<&Arc<RenderImage>>, size: (u32, u32), id: &'static str| -> AnyElement {
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
            };
        let images: Vec<AnyElement> = match &loaded {
            None => Vec::new(),
            Some(l) => {
                let i = &l.images;
                let (b, a) = (l.summary.before, l.summary.after);
                let union = (b.0.max(a.0), b.1.max(a.1));
                match view.mode {
                    Mode::Before => vec![picture(i.before.as_ref(), b, "image-before")],
                    Mode::After => vec![picture(i.after.as_ref(), a, "image-after")],
                    Mode::Diff => vec![picture(i.heat.as_ref(), union, "image-diff")],
                    Mode::SideBySide => vec![
                        picture(i.before.as_ref(), b, "image-before"),
                        picture(i.after.as_ref(), a, "image-after"),
                    ],
                }
            }
        };
        let bounds = self.bounds.clone();
        let paint_bounds = self.bounds.clone();
        div()
            .id("compare-panel")
            .track_focus(&self.focus)
            .key_context("GoldenReview")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(8.))
            .bg(body)
            .text_color(text)
            .text_sm()
            .child(toolbar)
            .child(div().id("diff-summary").test_support().child(summary))
            .child(div().id("pixel-readout").test_support().child(inspect))
            .child(
                div()
                    .id("compare-images")
                    .test_support()
                    .flex_1()
                    .overflow_scroll()
                    .relative()
                    .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
                        if let Some(b) = bounds.get() {
                            let x = f32::from(e.position.x - b.origin.x) / zoom;
                            let y = f32::from(e.position.y - b.origin.y) / zoom;
                            if x >= 0. && y >= 0. {
                                controls::dispatch(
                                    paths::INSPECT,
                                    names::SET_VALUE,
                                    Value::List(Arc::from([
                                        Value::Num(f64::from(x.floor())),
                                        Value::Num(f64::from(y.floor())),
                                    ])),
                                    Bus::Pointer,
                                    window,
                                    cx,
                                );
                            }
                        }
                    })
                    .child(div().flex().gap(px(8.)).children(images))
                    .child(
                        canvas(move |b, _, _| paint_bounds.set(Some(b)), |_, _, _, _| {})
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    ),
            )
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
            name: names::SET_VALUE.into(),
            payload: Value::Str(mode.wire().into()),
        };
        let id = format!("goldens.view-{}", mode.wire());
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
    dock: Entity<DockArea>,
    overlay: Entity<DockDragOverlay>,
    list: Entity<ListPanel>,
    compare: DockablePanel<ComparePanel>,
    _controls: Vec<Entity<controls::ReviewControl>>,
}

impl ReviewApp {
    pub fn new(store: Store<Review>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let controls = controls::register_all(&store, window, cx);
        let commands = Commands::new(cx);
        if let Err(e) = register_commands(&commands, cx) {
            store.dispatch(crate::review::Failed(format!("shortcuts: {e}").into()), cx);
        }
        let (dock, overlay) = guiraffe_dnd::dock::new_dock("goldens-dock", 1, window, cx);
        let list = cx.new(|cx| ListPanel::new(&store, window, cx));
        let compare = cx.new(|cx| ComparePanel::new(&store, cx));
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
        let focus = list.read(cx).focus.clone();
        window.focus(&focus, cx);
        Self {
            store,
            commands,
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

impl Render for ReviewApp {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CommandScope::new(&self.commands).size_full().child(
            div().size_full().relative().child(self.dock.clone()).child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(self.overlay.clone()),
            ),
        )
    }
}
