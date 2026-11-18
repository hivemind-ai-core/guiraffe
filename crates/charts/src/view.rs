use crate::{
    ChartSpec, DatumId, Domain, PreparedChart, Scene, SeriesId, paint,
    scene::{Hit, Position, Rect},
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, DispatchPhase, Entity, EventEmitter,
    HitboxBehavior, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, Role, ScrollEvent,
    StatefulInteractiveElement, StyleRefinement, Styled, WeakEntity, Window,
    base::{Disableable, Root, RootPlugin, TestSupportExt},
    canvas,
    component::{Sizable, button::Button},
    div, px,
};
use guiraffe_core::GuiraffeTheme;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const PAGE_SIZE: usize = 8;
const HIT_RADIUS: f32 = 24.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartEvent {
    Selected(DatumId),
    ViewportChanged(Domain),
    Brushed(Domain),
    SeriesVisibility { series: SeriesId, visible: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct ChartOptions {
    pub height: f32,
    pub legend: bool,
    pub crosshair: bool,
    pub tooltip: bool,
    pub navigation: bool,
    /// Enable tab stops for legend and data table buttons.
    pub keyboard: bool,
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self {
            height: 290.,
            legend: true,
            crosshair: true,
            tooltip: true,
            navigation: true,
            keyboard: true,
        }
    }
}

#[derive(Default)]
struct Frame {
    bounds: Bounds<Pixels>,
    scene: Option<(Domain, Scene)>,
    pending: Option<SceneRequest>,
    builds: usize,
    paints: usize,
}

struct SceneRequest(Arc<AtomicBool>);

impl Drop for SceneRequest {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct PlotState {
    chart: Arc<PreparedChart>,
    domain: Domain,
    hidden: BTreeSet<SeriesId>,
    focused: Option<SeriesId>,
    frame: Frame,
}

impl PlotState {
    fn invalidate(&mut self) {
        self.frame.scene = None;
        self.frame.pending = None;
    }

    fn prepare(&mut self, bounds: Bounds<Pixels>, ink: &WeakEntity<PlotInk>, cx: &mut App) {
        if self.frame.bounds.size != bounds.size {
            self.invalidate();
        }
        self.frame.bounds = bounds;
        if self
            .frame
            .scene
            .as_ref()
            .is_some_and(|(domain, _)| *domain == self.domain)
            || self.frame.pending.is_some()
        {
            return;
        }
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        if self.chart.len() < 100_000 {
            self.frame.scene = Some((
                self.domain,
                self.chart.scene(width, height, self.domain, &self.hidden),
            ));
            self.frame.builds += 1;
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.frame.pending = Some(SceneRequest(cancelled.clone()));
        let chart = self.chart.clone();
        let hidden = self.hidden.clone();
        let domain = self.domain;
        let ink = ink.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            let stop = cancelled.clone();
            let scene = executor
                .spawn(async move {
                    chart.scene_with_cancel(width, height, domain, &hidden, &|| {
                        stop.load(Ordering::Relaxed)
                    })
                })
                .await;
            let _ = ink.update(cx, |ink, cx| {
                let mut state = ink.0.borrow_mut();
                if state
                    .frame
                    .pending
                    .as_ref()
                    .is_some_and(|request| Arc::ptr_eq(&request.0, &cancelled))
                {
                    state.frame.pending = None;
                    state.frame.scene = Some((domain, scene));
                    state.frame.builds += 1;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn local(&self, pos: Point<Pixels>) -> Position {
        let delta = pos - self.frame.bounds.origin;
        (f32::from(delta.x), f32::from(delta.y))
    }
}

/// Install chart hover support before creating GPUI Kit window roots.
///
/// Upstream GPUI paints hover in a root layer so a cached embedding view
/// can stay cached. The fast backend retains the chart-local overlay.
pub fn init(cx: &mut App) {
    if !gpui_kit::GPUI_FAST {
        Root::register_plugin(cx, |_, _| ChartOverlayLayer::default());
    }
}

#[derive(Default)]
struct ChartOverlayLayer {
    overlays: Vec<WeakEntity<Overlay>>,
}

impl RootPlugin for ChartOverlayLayer {}

impl Render for ChartOverlayLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.overlays.retain(|overlay| overlay.upgrade().is_some());
        div().absolute().size_full().children(
            self.overlays
                .iter()
                .filter_map(WeakEntity::upgrade)
                .map(|overlay| overlay.cached(StyleRefinement::default().absolute().size_full())),
        )
    }
}

/// Owns persistent chart settings and a child entity for transient pointer state.
/// Prepare large models on a background executor and publish them through `replace`.
pub struct ChartView {
    plot: Rc<RefCell<PlotState>>,
    overlay: Entity<Overlay>,
    ink: Entity<PlotInk>,
    options: ChartOptions,
    table: Option<usize>,
    preparing: Option<SceneRequest>,
    window_overlay: bool,
    overlay_mount_checked: bool,
}

impl EventEmitter<ChartEvent> for ChartView {}

impl ChartView {
    pub fn new(chart: PreparedChart, options: ChartOptions, cx: &mut App) -> Entity<Self> {
        let plot = Rc::new(RefCell::new(PlotState {
            domain: chart.x_domain(),
            chart: Arc::new(chart),
            hidden: BTreeSet::new(),
            focused: None,
            frame: Frame::default(),
        }));
        cx.new(|cx| {
            let owner = cx.weak_entity();
            let overlay = cx.new(|_| Overlay {
                plot: plot.clone(),
                owner,
                options,
                hover: None,
                anchor: None,
                in_window_layer: false,
            });
            let ink = cx.new(|_| PlotInk(plot.clone(), overlay.downgrade()));
            Self {
                plot,
                ink,
                overlay,
                options,
                table: None,
                preparing: None,
                window_overlay: false,
                overlay_mount_checked: false,
            }
        })
    }

    /// Mount a bounded preview of supported marks while preparing the complete
    /// source. Use `new` when a prepared chart is already available.
    pub fn from_spec(spec: ChartSpec, options: ChartOptions, cx: &mut App) -> Entity<Self> {
        let view = Self::new(spec.preview(), options, cx);
        view.update(cx, |view, cx| {
            let cancelled = Arc::new(AtomicBool::new(false));
            view.preparing = Some(SceneRequest(cancelled.clone()));
            let executor = cx.background_executor().clone();
            cx.spawn(async move |view, cx| {
                let stop = cancelled.clone();
                let chart = executor
                    .spawn(
                        async move { spec.prepare_with_cancel(&|| stop.load(Ordering::Relaxed)) },
                    )
                    .await;
                let _ = view.update(cx, |view, cx| {
                    if !cancelled.load(Ordering::Relaxed) {
                        view.replace(chart, cx);
                    }
                });
            })
            .detach();
        });
        view
    }

    pub fn replace(&mut self, chart: PreparedChart, cx: &mut Context<Self>) {
        self.preparing = None;
        let mut plot = self.plot.borrow_mut();
        let following = plot.domain == plot.chart.x_domain();
        if following {
            plot.domain = chart.x_domain();
        }
        plot.hidden
            .retain(|id| chart.legend().iter().any(|entry| entry.id == *id));
        plot.focused = None;
        plot.chart = Arc::new(chart);
        plot.invalidate();
        drop(plot);
        App::notify(cx, self.ink.entity_id());
        self.clear_hover(cx);
        cx.notify();
    }

    pub fn viewport(&self) -> Domain {
        self.plot.borrow().domain
    }

    pub fn hovered(&self, cx: &App) -> Option<DatumId> {
        self.overlay
            .read(cx)
            .hover
            .as_ref()?
            .hit
            .as_ref()
            .map(|hit| hit.id)
    }

    pub fn series_visible(&self, id: SeriesId) -> bool {
        !self.plot.borrow().hidden.contains(&id)
    }

    /// Number of viewport scene compilations since mounting.
    pub fn scene_build_count(&self) -> usize {
        self.plot.borrow().frame.builds
    }

    /// Number of static canvas paints since mounting.
    pub fn canvas_paint_count(&self) -> usize {
        self.plot.borrow().frame.paints
    }

    pub fn toggle_series(&mut self, series: SeriesId, cx: &mut Context<Self>) {
        let mut plot = self.plot.borrow_mut();
        if !plot.chart.legend().iter().any(|entry| entry.id == series) {
            return;
        }
        let visible = plot.hidden.remove(&series);
        if !visible {
            plot.hidden.insert(series);
            if plot.focused == Some(series) {
                plot.focused = None;
            }
        }
        plot.invalidate();
        drop(plot);
        App::notify(cx, self.ink.entity_id());
        self.clear_hover(cx);
        cx.emit(ChartEvent::SeriesVisibility { series, visible });
        cx.notify();
    }

    fn focus_series(&mut self, series: SeriesId, inside: bool, cx: &mut Context<Self>) {
        let mut state = self.plot.borrow_mut();
        let previous = state.focused;
        if inside && !state.hidden.contains(&series) {
            state.focused = Some(series);
        } else if state.focused == Some(series) {
            state.focused = None;
        }
        if state.focused != previous {
            App::notify(cx, self.ink.entity_id());
        }
    }

    fn clear_hover(&self, cx: &mut Context<Self>) {
        self.overlay.update(cx, |overlay, cx| {
            overlay.hover = None;
            overlay.anchor = None;
            cx.notify();
        });
    }

    fn navigate(&mut self, domain: Domain, brush: bool, cx: &mut Context<Self>) {
        let mut plot = self.plot.borrow_mut();
        let Some(domain) =
            constrained_domain(&plot.chart.axes[0].scale, plot.chart.x_domain(), domain)
        else {
            return;
        };
        if plot.domain == domain {
            return;
        }
        plot.domain = domain;
        plot.frame.pending = None;
        drop(plot);
        App::notify(cx, self.ink.entity_id());
        // Gesture callbacks already hold the overlay entity through Context::listener.
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |view, cx| view.clear_hover(cx));
        });
        cx.emit(ChartEvent::ViewportChanged(domain));
        if brush {
            cx.emit(ChartEvent::Brushed(domain));
        }
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        let domain = self.plot.borrow().chart.x_domain();
        self.navigate(domain, false, cx);
    }

    fn table(&self, page: usize) -> AnyElement {
        let plot = self.plot.borrow();
        let model = &plot.chart;
        let mut skip = page.saturating_mul(PAGE_SIZE);
        let mut left = PAGE_SIZE;
        let mut table = div()
            .id("chart-data-table")
            .test_support()
            .role(Role::Table)
            .aria_label("Chart data")
            .text_sm()
            .child(table_row(
                ["Series".into(), "X".into(), "Y".into(), "Key".into()],
                0,
            ));
        for run in &model.runs {
            if skip >= run.rows.len() {
                skip -= run.rows.len();
                continue;
            }
            let y = if run.secondary { 2 } else { 1 };
            let name = model
                .legend()
                .get(run.palette as usize)
                .map_or("", |s| s.label.as_str());
            for row in run.rows.iter().skip(skip).take(left) {
                table = table.child(table_row(
                    [
                        name.to_owned(),
                        model.axes[0].scale.format(row.x),
                        model.axes[y].scale.format(row.y),
                        row.id.key.to_string(),
                    ],
                    PAGE_SIZE - left + 1,
                ));
                left -= 1;
            }
            if left == 0 {
                break;
            }
            skip = 0;
        }
        table.into_any_element()
    }
}

fn table_row(values: [String; 4], row: usize) -> impl IntoElement {
    div()
        .id(("chart-row", row))
        .role(Role::Row)
        .flex()
        .gap(px(8.))
        .children(values.into_iter().enumerate().map(|(column, value)| {
            div()
                .id(("chart-cell", column))
                .flex_1()
                .role(if row == 0 {
                    Role::ColumnHeader
                } else {
                    Role::Cell
                })
                .child(value)
        }))
}

impl Render for ChartView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !gpui_kit::GPUI_FAST && !self.overlay_mount_checked {
            self.overlay_mount_checked = true;
            if let Some(root) = window.root::<Root>().flatten() {
                let chart = cx.weak_entity();
                cx.defer(move |cx| {
                    let Some(layer) = root.read(cx).plugin::<ChartOverlayLayer>() else {
                        return;
                    };
                    let Ok(overlay) = chart.update(cx, |chart, cx| {
                        chart.window_overlay = true;
                        chart
                            .overlay
                            .update(cx, |overlay, _| overlay.in_window_layer = true);
                        cx.notify();
                        chart.overlay.downgrade()
                    }) else {
                        return;
                    };
                    layer.update(cx, |layer, cx| {
                        layer.overlays.push(overlay);
                        cx.notify();
                    });
                });
            }
        }
        let theme = GuiraffeTheme::global(cx).clone();
        let surfaces = theme.surfaces();
        let palette = paint::categorical_palette(&theme);
        let plot = self.plot.borrow();
        let label = if plot.chart.summary.is_empty() {
            "Chart"
        } else {
            &plot.chart.summary
        };
        let pages = plot.chart.len().div_ceil(PAGE_SIZE).max(1);
        let table_page = self.table.map(|p| p.min(pages - 1));
        let label = format!("{label} · {} observations", plot.chart.len());
        let mut controls = div().flex().flex_wrap().gap(px(5.));
        if self.options.legend {
            for entry in plot.chart.legend() {
                let series = entry.id;
                let visible = !plot.hidden.contains(&series);
                let ink = palette[entry.palette as usize % palette.len()].to_hsla();
                let swatch = div().size(px(8.)).rounded_full();
                let swatch = if visible {
                    swatch.bg(ink)
                } else {
                    swatch.border_1().border_color(ink)
                };
                let label = if plot.chart.has_right_axis {
                    let axis = if entry.secondary {
                        "right axis"
                    } else {
                        "left axis"
                    };
                    format!("{} · {axis}", entry.label)
                } else {
                    entry.label.clone()
                };
                controls = controls.child(
                    Button::new(("chart-series", entry.palette as usize))
                        .accessibility_id(format!("chart-series-{}", entry.palette))
                        .accessibility_label(label.clone())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .child(swatch)
                                .child(label),
                        )
                        .toggled(visible)
                        .small()
                        .tab_stop(self.options.keyboard)
                        .on_hover(cx.listener(move |this, inside: &bool, _, cx| {
                            this.focus_series(series, *inside, cx)
                        }))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.toggle_series(series, cx)),
                        ),
                );
            }
        }
        controls = controls.child(
            Button::new("chart-table-toggle")
                .label(if table_page.is_some() {
                    "Show chart"
                } else {
                    "View data"
                })
                .small()
                .tab_stop(self.options.keyboard)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.table = if this.table.is_some() { None } else { Some(0) };
                    this.clear_hover(cx);
                    cx.notify();
                })),
        );
        if self.options.navigation && plot.domain != plot.chart.x_domain() && table_page.is_none() {
            controls = controls.child(
                Button::new("chart-reset")
                    .label("Reset view")
                    .small()
                    .tab_stop(self.options.keyboard)
                    .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
            );
        }
        drop(plot);
        let body = if let Some(page) = table_page {
            let page_button = |id, text, disabled, target| {
                Button::new(id)
                    .label(text)
                    .small()
                    .disabled(disabled)
                    .tab_stop(self.options.keyboard)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.table = Some(target);
                        cx.notify();
                    }))
            };
            div()
                .px(px(8.))
                .child(self.table(page))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(page_button(
                            "chart-prev",
                            "Previous",
                            page == 0,
                            page.saturating_sub(1),
                        ))
                        .child(format!("Page {} of {pages}", page + 1))
                        .child(page_button(
                            "chart-next",
                            "Next",
                            page + 1 == pages,
                            page + 1,
                        )),
                )
                .into_any_element()
        } else {
            div()
                .id("chart-canvas")
                .test_support()
                .role(Role::Canvas)
                .aria_label(label)
                .h(px(self.options.height.max(64.)))
                .w_full()
                .relative()
                .overflow_hidden()
                .bg(surfaces.body.to_hsla())
                .child(
                    self.ink
                        .clone()
                        .cached(StyleRefinement::default().absolute().size_full()),
                )
                .child(
                    div()
                        .id("chart-interaction")
                        .absolute()
                        .size_full()
                        .child(pointer_motion(&self.overlay))
                        .on_mouse_down(
                            MouseButton::Left,
                            overlay_listener(&self.overlay, |overlay, event, _| {
                                overlay.press(event)
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            overlay_listener(&self.overlay, Overlay::release),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            overlay_listener(&self.overlay, Overlay::release),
                        )
                        .on_scroll(overlay_listener(&self.overlay, Overlay::scroll)),
                )
                .children((!self.window_overlay).then(|| self.overlay.clone()))
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(6.))
            .text_color(surfaces.text.to_hsla())
            .child(controls)
            .children(
                self.preparing
                    .as_ref()
                    .map(|_| div().child("Preparing full chart…")),
            )
            .child(body)
    }
}

fn pointer_motion(overlay: &Entity<Overlay>) -> impl IntoElement {
    let overlay = overlay.downgrade();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let motion = overlay.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = motion.update(cx, |overlay, cx| {
                        if hitbox.is_hovered(window) {
                            overlay.motion(event, cx);
                        } else {
                            overlay.leave(cx);
                        }
                    });
                }
            });
            let exit = overlay.clone();
            window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = exit.update(cx, Overlay::leave);
                }
            });
        },
    )
    .absolute()
    .size_full()
}

fn overlay_listener<E>(
    overlay: &Entity<Overlay>,
    handle: impl Fn(&mut Overlay, &E, &mut Context<Overlay>) + 'static,
) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let overlay = overlay.downgrade();
    move |event, _, cx| {
        let _ = overlay.update(cx, |overlay, cx| handle(overlay, event, cx));
    }
}

struct PlotInk(Rc<RefCell<PlotState>>, WeakEntity<Overlay>);

impl Render for PlotInk {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let layout = self.0.clone();
        let paint = self.0.clone();
        let overlay = self.1.clone();
        let ink = cx.weak_entity();
        canvas(
            move |bounds, window, cx| {
                let mut layout = layout.borrow_mut();
                if layout.frame.bounds != bounds
                    && let Some(overlay) = overlay.upgrade()
                {
                    window.defer(cx, move |_, cx| App::notify(cx, overlay.entity_id()));
                }
                layout.prepare(bounds, &ink, cx);
            },
            move |bounds, _, window, cx| {
                let mut state = paint.borrow_mut();
                state.frame.paints += 1;
                if let Some((domain, scene)) = &state.frame.scene {
                    paint::paint_scene(
                        paint::PaintScene {
                            scene,
                            chart: &state.chart,
                            domain: *domain,
                            theme: &theme,
                            origin: bounds.origin,
                            focused: state.focused,
                        },
                        window,
                        cx,
                    );
                }
            },
        )
        .size_full()
    }
}

#[derive(PartialEq)]
struct Hover {
    pos: Position,
    rect: Rect,
    facet: u32,
    hit: Option<Hit>,
}

struct Overlay {
    plot: Rc<RefCell<PlotState>>,
    owner: WeakEntity<ChartView>,
    options: ChartOptions,
    hover: Option<Hover>,
    anchor: Option<(u32, f32)>,
    in_window_layer: bool,
}

impl Overlay {
    fn locate(&self, point: Point<Pixels>) -> Option<Hover> {
        let state = self.plot.borrow();
        let pos = state.local(point);
        let (domain, scene) = state.frame.scene.as_ref()?;
        if *domain != state.domain {
            return None;
        }
        let panel = scene.panel(pos)?;
        let hit = state
            .chart
            .nearest(panel, state.domain, &state.hidden, pos, HIT_RADIUS);
        Some(Hover {
            pos,
            rect: panel.rect,
            facet: panel.facet,
            hit,
        })
    }

    fn motion(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.options.tooltip && !self.options.crosshair && self.anchor.is_none() {
            return;
        }
        let mut next = self.locate(event.position);
        if self.anchor.is_none()
            && let Some(hover) = &mut next
            && let Some(hit) = &hover.hit
        {
            hover.pos = hit.point;
        }
        if self.hover != next {
            self.hover = next;
            cx.notify();
        }
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        if self.hover.take().is_some() {
            cx.notify();
        }
    }

    fn press(&mut self, event: &MouseDownEvent) {
        let state = self.plot.borrow();
        let pos = state.local(event.position);
        self.anchor = state
            .frame
            .scene
            .as_ref()
            .filter(|(domain, _)| *domain == state.domain)
            .and_then(|(_, scene)| scene.panel(pos))
            .map(|panel| (panel.facet, pos.0));
    }

    fn release(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(anchor) = self.anchor.take() else {
            return;
        };
        let state = self.plot.borrow();
        let pos = state.local(event.position);
        let domain = state.domain;
        let scale = &state.chart.axes[0].scale;
        let brush = self
            .options
            .navigation
            .then_some(anchor)
            .and_then(|(facet, start)| {
                let panel = state
                    .frame
                    .scene
                    .as_ref()
                    .filter(|(rendered, _)| *rendered == domain)?
                    .1
                    .panels
                    .iter()
                    .find(|p| p.facet == facet)?;
                let end = pos.0.clamp(panel.rect.left, panel.rect.right);
                if (start - end).abs() < 6. {
                    return None;
                }
                Domain::new(
                    scale.invert(domain, panel.rect.fraction(start.min(end))),
                    scale.invert(domain, panel.rect.fraction(start.max(end))),
                )
            });
        drop(state);
        if let Some(domain) = brush {
            let _ = self
                .owner
                .update(cx, |view, cx| view.navigate(domain, true, cx));
        } else if let Some(hit) = self
            .locate(event.position)
            .filter(|hover| hover.facet == anchor.0)
            .and_then(|hover| hover.hit)
        {
            let _ = self
                .owner
                .update(cx, |_, cx| cx.emit(ChartEvent::Selected(hit.id)));
        }
        cx.notify();
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        if !self.options.navigation {
            return;
        }
        let state = self.plot.borrow();
        let pos = state.local(event.position);
        let Some(panel) = state
            .frame
            .scene
            .as_ref()
            .and_then(|(_, scene)| scene.panel(pos))
        else {
            return;
        };
        let delta = event.delta.pixel_delta(px(24.));
        let dx = f64::from(f32::from(delta.x));
        let dy = f64::from(f32::from(delta.y));
        let (min, max) = if dx.abs() > dy.abs() {
            let shift = -dx / f64::from(panel.rect.width());
            (shift, 1. + shift)
        } else {
            let anchor = panel.rect.fraction(pos.0);
            let factor = (dy * -0.006).clamp(-2., 2.).exp();
            (anchor * (1. - factor), anchor + (1. - anchor) * factor)
        };
        let scale = &state.chart.axes[0].scale;
        let domain = Domain::new(
            scale.invert(state.domain, min),
            scale.invert(state.domain, max),
        );
        drop(state);
        if let Some(domain) = domain {
            let _ = self
                .owner
                .update(cx, |view, cx| view.navigate(domain, false, cx));
            cx.stop_propagation();
        }
    }
}

impl Render for Overlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let surfaces = theme.surfaces();
        let selected = theme
            .variant(None, guiraffe_core::Variant::Filled)
            .background;
        let mut overlay = div().absolute().overflow_hidden();
        if self.in_window_layer {
            let bounds = self.plot.borrow().frame.bounds;
            overlay = overlay
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height);
        } else {
            overlay = overlay.size_full();
        }
        if let Some(hover) = &self.hover {
            let rect = hover.rect;
            let (x, y) = hover.hit.as_ref().map_or(hover.pos, |hit| hit.point);
            let drag = self
                .anchor
                .filter(|(facet, _)| self.options.navigation && *facet == hover.facet)
                .map(|(_, start)| (start, hover.pos.0));
            let crosshair = self.options.crosshair;
            overlay = overlay.child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        if crosshair {
                            paint::segment(
                                window,
                                b.origin,
                                (x, rect.top),
                                (x, rect.bottom),
                                surfaces.dimmed.to_hsla(),
                            );
                            paint::segment(
                                window,
                                b.origin,
                                (rect.left, y),
                                (rect.right, y),
                                surfaces.dimmed.to_hsla(),
                            );
                        }
                        if let Some((start, end)) = drag {
                            paint::box_fill(
                                window,
                                b.origin,
                                (start, rect.top),
                                (end, rect.bottom),
                                selected.alpha(0.18).to_hsla(),
                                0.,
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
            if self.options.tooltip
                && let Some(hit) = &hover.hit
            {
                let plot = self.plot.borrow();
                let series = plot.chart.legend().iter().find(|s| s.id == hit.id.series);
                let name = series.map_or("", |s| s.label.as_str());
                let y_axis = if hit.secondary { 2 } else { 1 };
                let values = format!(
                    "{} · {}",
                    plot.chart.axes[0].scale.format(hit.x),
                    plot.chart.axes[y_axis].scale.format(hit.y)
                );
                let tip_x = (x + 10.).min((rect.right - 148.).max(rect.left));
                let tip_y = (y - 54.).max(rect.top + 4.);
                overlay = overlay.child(
                    div()
                        .id("chart-tooltip")
                        .test_support()
                        .role(Role::Tooltip)
                        .aria_label(format!("{name}, {values}"))
                        .absolute()
                        .left(px(tip_x))
                        .top(px(tip_y))
                        .min_w(px(120.))
                        .px(px(8.))
                        .py(px(4.))
                        .rounded_md()
                        .bg(surfaces.raised.to_hsla())
                        .border_1()
                        .border_color(surfaces.border.to_hsla())
                        .text_color(surfaces.text.to_hsla())
                        .text_xs()
                        .child(div().child(name.to_owned()))
                        .child(div().text_color(surfaces.dimmed.to_hsla()).child(values)),
                );
            }
        }
        overlay
    }
}

// Work in projected coordinates so log axes retain a positive minimum.
fn constrained_domain(scale: &crate::Scale, full: Domain, requested: Domain) -> Option<Domain> {
    const SLACK: f64 = 0.05;
    const MIN_SPAN: f64 = 0.0001;
    if requested == full {
        return Some(full);
    }
    let min = scale.project(full, requested.min())?;
    let max = scale.project(full, requested.max())?;
    if min >= -SLACK && max <= 1. + SLACK && max - min >= MIN_SPAN {
        return Some(requested);
    }
    let span = (max - min).clamp(MIN_SPAN, 1. + 2. * SLACK);
    let start = (min + ((max - min) - span) * 0.5).clamp(-SLACK, (1. + SLACK - span).max(-SLACK));
    Domain::new(scale.invert(full, start), scale.invert(full, start + span))
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    use crate::Scale;

    #[test]
    fn viewport_has_bounded_pan_and_zoom() {
        let full = Domain::new(0., 100.).expect("valid regression fixture");
        let outside = Domain::new(200., 220.).expect("valid regression fixture");
        let panned =
            constrained_domain(&Scale::Linear, full, outside).expect("valid regression fixture");
        assert!((panned.max() - 105.).abs() < 1e-9);
        assert!((panned.span() - 20.).abs() < 1e-9);
        let tiny = Domain::new(50., 50.00001).expect("valid regression fixture");
        let zoomed =
            constrained_domain(&Scale::Linear, full, tiny).expect("valid regression fixture");
        assert!((zoomed.span() - 0.01).abs() < 1e-9);
        let log = Domain::new(1., 1000.).expect("valid regression fixture");
        let zoomed = constrained_domain(&Scale::Log, log, tiny).expect("valid regression fixture");
        assert!(zoomed.min() > 0. && zoomed.span() > tiny.span());
    }
}
