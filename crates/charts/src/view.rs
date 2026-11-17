use crate::{
    DatumId, Domain, PreparedChart, Scene, SeriesId, paint,
    scene::{Hit, Position, Rect},
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, EventEmitter, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Point, Render, Role, ScrollEvent, StatefulInteractiveElement, StyleRefinement, Styled,
    WeakEntity, Window,
    base::{Disableable, TestSupportExt},
    canvas,
    component::{Sizable, button::Button},
    div, px,
};
use guiraffe_core::GuiraffeTheme;
use std::{cell::RefCell, collections::BTreeSet, rc::Rc, sync::Arc};

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
    /// Opt-in tab stops for legend and data table buttons.
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
            keyboard: false,
        }
    }
}

#[derive(Default)]
struct Frame {
    bounds: Bounds<Pixels>,
    scene: Option<Scene>,
    builds: usize,
    paints: usize,
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
    }

    fn prepare(&mut self, bounds: Bounds<Pixels>) {
        if self.frame.bounds.size != bounds.size {
            self.invalidate();
        }
        self.frame.bounds = bounds;
        if self.frame.scene.is_none() {
            self.frame.scene = Some(self.chart.scene(
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                self.domain,
                &self.hidden,
            ));
            self.frame.builds += 1;
        }
    }

    fn local(&self, pos: Point<Pixels>) -> Position {
        let delta = pos - self.frame.bounds.origin;
        (f32::from(delta.x), f32::from(delta.y))
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
            });
            let ink = cx.new(|_| PlotInk(plot.clone()));
            Self {
                plot,
                ink,
                overlay,
                options,
                table: None,
            }
        })
    }

    pub fn replace(&mut self, chart: PreparedChart, cx: &mut Context<Self>) {
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
        if plot.domain == domain {
            return;
        }
        plot.domain = domain;
        plot.invalidate();
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                let name = if visible {
                    entry.label.clone()
                } else {
                    format!("{} (hidden)", entry.label)
                };
                controls = controls.child(
                    Button::new(("chart-series", entry.palette as usize))
                        .accessibility_id(format!("chart-series-{}", entry.palette))
                        .label(name)
                        .child(div().size(px(8.)).rounded_full().bg(ink))
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
                .child(self.overlay.clone())
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(6.))
            .text_color(surfaces.text.to_hsla())
            .child(controls)
            .child(body)
    }
}

struct PlotInk(Rc<RefCell<PlotState>>);

impl Render for PlotInk {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let layout = self.0.clone();
        let paint = self.0.clone();
        canvas(
            move |bounds, _, _| layout.borrow_mut().prepare(bounds),
            move |bounds, _, window, cx| {
                let mut state = paint.borrow_mut();
                state.frame.paints += 1;
                if let Some(scene) = &state.frame.scene {
                    paint::paint_scene(
                        paint::PaintScene {
                            scene,
                            chart: &state.chart,
                            domain: state.domain,
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

struct Hover {
    pos: Position,
    rect: Rect,
    hit: Option<Hit>,
}

struct Overlay {
    plot: Rc<RefCell<PlotState>>,
    owner: WeakEntity<ChartView>,
    options: ChartOptions,
    hover: Option<Hover>,
    anchor: Option<(u32, f32)>,
}

impl Overlay {
    fn locate(&self, point: Point<Pixels>) -> Option<Hover> {
        let state = self.plot.borrow();
        let pos = state.local(point);
        let scene = state.frame.scene.as_ref()?;
        let panel = scene.panel(pos)?;
        let hit = state
            .chart
            .nearest(panel, state.domain, &state.hidden, pos, HIT_RADIUS);
        Some(Hover {
            pos,
            rect: panel.rect,
            hit,
        })
    }

    fn motion(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let next = self.locate(event.position);
        if self.hover.is_some() || next.is_some() {
            self.hover = next;
            cx.notify();
        }
    }

    fn press(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if self.options.navigation {
            let state = self.plot.borrow();
            let pos = state.local(event.position);
            if let Some(panel) = state
                .frame
                .scene
                .as_ref()
                .and_then(|scene| scene.panel(pos))
            {
                self.anchor = Some((panel.facet, pos.0));
                cx.notify();
            }
        }
    }

    fn release(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let anchor = self.anchor.take();
        let state = self.plot.borrow();
        let pos = state.local(event.position);
        let domain = state.domain;
        let scale = &state.chart.axes[0].scale;
        let brush = anchor.and_then(|(facet, start)| {
            let panel = state
                .frame
                .scene
                .as_ref()?
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
        } else if let Some(hit) = self.locate(event.position).and_then(|h| h.hit) {
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
            .and_then(|scene| scene.panel(pos))
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
        let mut overlay = div()
            .id("chart-interaction")
            .absolute()
            .size_full()
            .on_mouse_move(cx.listener(|this, event, _, cx| this.motion(event, cx)))
            .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                if !inside && this.hover.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.press(event, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.release(event, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| {
                    if this.anchor.is_some() {
                        this.release(event, cx);
                    }
                }),
            )
            .on_scroll(cx.listener(|this, event, _, cx| this.scroll(event, cx)));
        if let Some(hover) = &self.hover {
            let rect = hover.rect;
            let (x, y) = hover.hit.as_ref().map_or(hover.pos, |hit| hit.point);
            let drag = self.anchor.map(|(_, start)| (start, hover.pos.0));
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
