use crate::strips::{Projection, RowStrips};
use crate::{FlameLayout, Frame, FrameClicked, FrameId, FrameSource, LayoutError, Search};
use gpui_kit::{
    App, AppContext, Bounds, ContentMask, Context, DispatchPhase, Entity, EventEmitter,
    FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels,
    Point, Render, Role, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Task,
    TestSupportExt, WeakEntity, Window,
    base::{Root, RootPlugin},
    canvas,
    component::button::Button,
    div, fill, point, px, size,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Variant};
use std::{
    cell::RefCell,
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const ROW_HEIGHT: f64 = 21.;
const KEYBOARD_HELP: &str = "Use arrow keys to explore frames and Home to reset";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Flame,
    Icicle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColourScheme {
    #[default]
    Name,
    Package,
    Heat,
}

const LABEL_SLOTS: usize = 128;
const MAX_CACHED_LABEL_BYTES: usize = 256;

/// Retain the text handles passed to GPUI's line layout cache between paints.
struct LabelCache([Option<SharedString>; LABEL_SLOTS]);

impl LabelCache {
    fn text(&mut self, label: &str) -> SharedString {
        // Short strings fit inline in GPUI's SharedString.
        if label.len() <= 16 || label.len() > MAX_CACHED_LABEL_BYTES {
            return SharedString::from(label);
        }
        let mut hash = DefaultHasher::new();
        label.hash(&mut hash);
        self.text_in_slot(label, hash.finish() as usize % LABEL_SLOTS)
    }

    fn text_in_slot(&mut self, label: &str, slot: usize) -> SharedString {
        let entry = &mut self.0[slot];
        if let Some(text) = entry
            && text.as_ref() == label
        {
            return text.clone();
        }
        let text = SharedString::from(label);
        *entry = Some(text.clone());
        text
    }
}

const PREVIEW_FRAMES: usize = 4096;

struct Prefix {
    source: Arc<dyn FrameSource>,
    len: usize,
}

impl FrameSource for Prefix {
    fn len(&self) -> usize {
        self.len
    }

    fn frame(&self, index: usize) -> Frame<'_> {
        self.source.frame(index)
    }
}

struct PendingLayout {
    cancelled: Arc<AtomicBool>,
    _task: Task<()>,
    search: String,
}

impl Drop for PendingLayout {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub(super) fn palette_index(frame: Frame<'_>, scheme: ColourScheme) -> usize {
    if scheme == ColourScheme::Heat {
        return ((frame.self_value / frame.total) * 5.) as usize;
    }
    let mut hash = DefaultHasher::new();
    if scheme == ColourScheme::Package {
        frame.package
    } else {
        frame.label
    }
    .hash(&mut hash);
    hash.finish() as usize % 6
}

/// Flame and icicle plot. Tab into the plot, use the arrows to move among
/// frames, Enter or Space to activate a frame, and Home or Escape to reset.
pub struct FlameGraph {
    strips: Option<Rc<RefCell<RowStrips>>>,
    layout: Arc<FlameLayout>,
    domain: Range<f64>,
    focus: Option<FrameId>,
    hover: Option<FrameId>,
    keyboard_focus: Option<FocusHandle>,
    accessible_frame: SharedString,
    tooltip: Option<Entity<Tooltip>>,
    window_tooltip: bool,
    drag: Option<(Point<Pixels>, bool)>,
    search: Option<Arc<Search>>,
    search_task: Option<Task<()>>,
    orientation: Orientation,
    colour: ColourScheme,
    bounds: Bounds<Pixels>,
    first_row: usize,
    labels: Rc<RefCell<LabelCache>>,
    pending_layout: Option<PendingLayout>,
    load_failure: Option<LayoutError>,
}

impl EventEmitter<FrameClicked> for FlameGraph {}

impl FlameGraph {
    pub fn new(layout: Arc<FlameLayout>) -> Self {
        Self {
            strips: None,
            domain: layout.domain(),
            layout,
            focus: None,
            hover: None,
            keyboard_focus: None,
            accessible_frame: KEYBOARD_HELP.into(),
            tooltip: None,
            window_tooltip: false,
            drag: None,
            search: None,
            search_task: None,
            orientation: Orientation::Flame,
            colour: ColourScheme::Name,
            bounds: Bounds::default(),
            first_row: 0,
            labels: Rc::new(RefCell::new(LabelCache(std::array::from_fn(|_| None)))),
            pending_layout: None,
            load_failure: None,
        }
    }

    /// Mount a bounded preview of the first stacks and prepare the complete
    /// layout on the background executor. The preview advertises its coverage
    /// and the full domain replaces it automatically after validation.
    pub fn from_source(
        source: Arc<dyn FrameSource>,
        cx: &mut Context<Self>,
    ) -> Result<Self, LayoutError> {
        if source.len() <= PREVIEW_FRAMES {
            return Ok(Self::new(Arc::new(FlameLayout::new(source)?)));
        }
        let preview = FlameLayout::new(Arc::new(Prefix {
            source: source.clone(),
            len: PREVIEW_FRAMES,
        }))?;
        let mut graph = Self::new(Arc::new(preview));
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let task = cx.background_executor().spawn(async move {
            match FlameLayout::build(source, || signal.load(Ordering::Relaxed)) {
                Ok((layout, true)) => Some(Ok(Arc::new(layout))),
                Ok((_, false)) => None,
                Err(error) => Some(Err(error)),
            }
        });
        let task = cx.spawn(async move |graph, cx| {
            let Some(result) = task.await else {
                return;
            };
            let _ = graph.update(cx, |graph, cx| {
                let search = graph
                    .pending_layout
                    .take()
                    .map(|mut pending| std::mem::take(&mut pending.search));
                match result {
                    Ok(layout) => {
                        graph.layout = layout;
                        graph.drag = None;
                        graph.reset(cx);
                        graph.set_search(search.as_deref().unwrap_or_default(), cx);
                    }
                    Err(error) => {
                        graph.load_failure = Some(error);
                        cx.notify();
                    }
                }
            });
        });
        graph.pending_layout = Some(PendingLayout {
            cancelled,
            _task: task,
            search: String::new(),
        });
        Ok(graph)
    }

    /// Whether the full source is still being prepared.
    pub fn is_loading(&self) -> bool {
        self.pending_layout.is_some()
    }

    /// Validation error reported by the deferred layout, if any.
    pub fn load_failure(&self) -> Option<LayoutError> {
        self.load_failure
    }

    /// Height of the plot area needed to display the full profile at once.
    /// The controls above the plot take additional space.
    pub fn full_plot_height(&self) -> Pixels {
        px(self.layout.rows() as f32 * ROW_HEIGHT as f32)
    }

    pub fn set_search(&mut self, query: &str, cx: &mut Context<Self>) {
        if let Some(pending) = &mut self.pending_layout {
            pending.search.clear();
            pending.search.push_str(query);
        }
        self.search_task = None;
        if self.search.take().is_some() {
            cx.notify();
        }
        if query.is_empty() {
            return;
        }
        let query = query.to_owned();
        let layout = self.layout.clone();
        let task = cx
            .background_executor()
            .spawn(async move { layout.search(&query).map(Arc::new) });
        self.search_task = Some(cx.spawn(async move |graph, cx| {
            let search = task.await;
            let _ = graph.update(cx, |graph, cx| {
                graph.search = search;
                cx.notify();
            });
        }));
    }

    pub fn set_orientation(&mut self, orientation: Orientation, cx: &mut Context<Self>) {
        self.orientation = orientation;
        self.set_hover(None, cx);
        cx.notify();
    }

    pub fn set_colour_scheme(&mut self, colour: ColourScheme, cx: &mut Context<Self>) {
        self.colour = colour;
        cx.notify();
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.domain = self.layout.domain();
        self.first_row = 0;
        self.set_hover(None, cx);
        self.focus = None;
        self.accessible_frame = KEYBOARD_HELP.into();
        cx.notify();
    }

    fn set_hover(&mut self, id: Option<FrameId>, cx: &mut Context<Self>) {
        if self.hover == id {
            return;
        }
        self.hover = id;
        if let Some(tooltip) = &self.tooltip {
            let text = id.and_then(|id| self.layout.frame(id)).map(|frame| {
                format!(
                    "{} · self {:.3} · total {:.3}",
                    frame.label, frame.self_value, frame.total
                )
            });
            tooltip.update(cx, |tooltip, cx| {
                tooltip.text = text;
                tooltip.anchor = point(self.bounds.right() - px(6.), self.bounds.top() + px(6.));
                cx.notify();
            });
        }
    }

    fn motion(&mut self, event: &gpui_kit::MouseMoveEvent, cx: &mut Context<Self>) {
        if let Some((previous, moved)) = self.drag {
            let dx = f64::from(f32::from(event.position.x - previous.x));
            if self.width() > 0. && (moved || dx.abs() >= 3.) {
                let span = self.domain.end - self.domain.start;
                let changed = self.set_domain(self.domain.start - dx * span / self.width(), span);
                self.drag = Some((event.position, true));
                if changed {
                    cx.notify();
                }
            }
        }
        self.set_hover(self.hit(event.position), cx);
    }

    fn width(&self) -> f64 {
        f32::from(self.bounds.size.width).into()
    }

    fn fraction(&self, position: Point<Pixels>) -> f64 {
        (f64::from(f32::from(position.x - self.bounds.origin.x)) / self.width()).clamp(0., 1.)
    }

    fn hit(&self, position: Point<Pixels>) -> Option<FrameId> {
        if !self.bounds.contains(&position) || self.width() <= 0. {
            return None;
        }
        let y = match self.orientation {
            Orientation::Flame => self.bounds.bottom() - position.y,
            Orientation::Icicle => position.y - self.bounds.top(),
        };
        let row = (f64::from(f32::from(y)) / ROW_HEIGHT).floor() as usize;
        let x = self.domain.start + self.fraction(position) * (self.domain.end - self.domain.start);
        self.layout.hit(self.first_row + row, x)
    }

    fn set_domain(&mut self, start: f64, span: f64) -> bool {
        let total = self.layout.domain().end;
        if total > 0. && span.is_finite() && start.is_finite() {
            let span = span.clamp(total * 1e-12, total);
            let start = start.clamp(0., total - span);
            let next = start..start + span;
            if next != self.domain {
                self.domain = next;
                return true;
            }
        }
        false
    }

    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let next = match key {
            "escape" | "home" => {
                self.reset(cx);
                None
            }
            "left" | "right" => {
                if let Some(id) = self.focus {
                    self.layout.adjacent_sibling(id, key == "right")
                } else {
                    self.layout.first_frame()
                }
            }
            "up" | "down" => {
                if let Some(id) = self.focus {
                    if (key == "up") == (self.orientation == Orientation::Flame) {
                        self.layout.first_child(id)
                    } else {
                        self.layout.frame(id).and_then(|frame| frame.parent)
                    }
                } else {
                    self.layout.first_frame()
                }
            }
            "enter" | "space" => {
                let id = self.focus.or_else(|| self.layout.first_frame());
                if let Some(clicked) = id.and_then(|id| self.layout.click(id)) {
                    cx.emit(clicked);
                }
                id.filter(|_| self.focus.is_none())
            }
            _ => return,
        };
        if let Some(id) = next {
            self.focus_frame(id, cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn focus_frame(&mut self, id: FrameId, cx: &mut Context<Self>) {
        if let (Some(domain), Some(event), Some(frame)) = (
            self.layout.focus(id),
            self.layout.click(id),
            self.layout.frame(id),
        ) {
            self.accessible_frame = format!(
                "{} · self {:.3} · total {:.3}",
                frame.label, frame.self_value, frame.total
            )
            .into();
            self.domain = domain;
            self.first_row = event.path.len() - 1;
            self.focus = Some(id);
            self.set_hover(None, cx);
        }
    }
}

impl Render for FlameGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let surface = theme.surfaces();
        let colors = match self.colour {
            ColourScheme::Heat => [
                ColorName::Blue,
                ColorName::Cyan,
                ColorName::Teal,
                ColorName::Yellow,
                ColorName::Orange,
                ColorName::Red,
            ],
            _ => [
                ColorName::Blue,
                ColorName::Cyan,
                ColorName::Teal,
                ColorName::Orange,
                ColorName::Grape,
                ColorName::Pink,
            ],
        }
        .map(|name| theme.variant(Some(name), Variant::Filled));
        let highlight = theme.variant(Some(theme.primary), Variant::Light);
        let breadcrumb = self.focus.and_then(|id| self.layout.click(id));
        let tooltip = self
            .tooltip
            .get_or_insert_with(|| {
                let layer = window
                    .root::<Root>()
                    .flatten()
                    .and_then(|root| root.read(cx).plugin::<TooltipLayer>());
                self.window_tooltip = layer.is_some();
                let tooltip = cx.new(|_| Tooltip {
                    text: None,
                    anchor: Point::default(),
                    global: self.window_tooltip,
                });
                if let Some(layer) = layer {
                    let weak = tooltip.downgrade();
                    cx.defer(move |cx| {
                        layer.update(cx, |layer, cx| {
                            layer.0.push(weak);
                            cx.notify();
                        })
                    });
                }
                tooltip
            })
            .clone();
        let strips = self
            .strips
            .get_or_insert_with(|| {
                let strips = Rc::<RefCell<RowStrips>>::default();
                let owned = strips.clone();
                cx.on_release(move |_, cx| {
                    owned.borrow_mut().release(cx);
                })
                .detach();
                strips
            })
            .clone();
        let keyboard_focus = self
            .keyboard_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let prepare = cx.entity().downgrade();
        let pointer = prepare.clone();
        let layout = self.layout.clone();
        let search = self.search.clone();
        let domain = self.domain.clone();
        let orientation = self.orientation;
        let first_row = self.first_row;
        let colour = self.colour;
        let labels = self.labels.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .text_color(surface.text.to_hsla())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        Button::new("flame-reset")
                            .accessibility_id("flame-reset")
                            .tab_stop(false)
                            .label("Reset")
                            .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                    )
                    .children(
                        breadcrumb
                            .into_iter()
                            .flat_map(|event| event.path)
                            .filter_map(|id| {
                                let label = SharedString::from(self.layout.frame(id)?.label);
                                Some(
                                    Button::new(("frame", id as usize))
                                        .tab_stop(false)
                                        .label(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.focus_frame(id, cx);
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
            .children(self.is_loading().then(|| {
                div().child(format!(
                    "Loading profile · showing first {PREVIEW_FRAMES} frames"
                ))
            }))
            .children(self.load_failure.map(|error| {
                div()
                    .id("flame-layout-error")
                    .role(Role::Alert)
                    .child(format!("Could not load full profile ({error:?})"))
            }))
            .child(
                div()
                    .id("flame-plot")
                    .test_support()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .cursor_grab()
                    .role(Role::Canvas)
                    .aria_label("Flame graph")
                    .aria_description(self.accessible_frame.clone())
                    .track_focus(&keyboard_focus)
                    .tab_stop(true)
                    .on_key_down(cx.listener(|this, event, _, cx| this.key(event, cx)))
                    .bg(surface.body.to_hsla())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, _| {
                            this.drag = Some((event.position, false))
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui_kit::MouseUpEvent, _, cx| {
                            if let Some((_, false)) = this.drag.take()
                                && let Some(id) = this.hit(event.position)
                                && let Some(clicked) = this.layout.click(id)
                            {
                                this.focus_frame(id, cx);
                                cx.emit(clicked);
                                cx.notify();
                            }
                        }),
                    )
                    .on_scroll(cx.listener(|this, event: &gpui_kit::ScrollEvent, _, cx| {
                        let delta = event.delta.pixel_delta(px(24.));
                        let span = this.domain.end - this.domain.start;
                        let anchor = this.fraction(event.position);
                        let next = span
                            * (-f64::from(f32::from(delta.y))
                                / if event.modifiers.control { 90. } else { 260. })
                            .exp();
                        let changed = this.set_domain(
                            this.domain.start + anchor * (span - next)
                                - f64::from(f32::from(delta.x)) * span / this.width(),
                            next,
                        );
                        this.set_hover(this.hit(event.position), cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let _ = prepare.update(cx, |this, _| this.bounds = bounds);
                            },
                            move |bounds, _, window, cx| {
                                let mut labels = labels.borrow_mut();
                                let height = f64::from(f32::from(bounds.size.height));
                                let width = f64::from(f32::from(bounds.size.width));
                                let slots = (height / ROW_HEIGHT).ceil() as usize;
                                // Frame rectangles occupy disjoint rows and columns. GPUI can
                                // order their quads together, then draw clipped labels above.
                                window.paint_layer(bounds, |window| {
                                    let mut strips = strips.borrow_mut();
                                    strips.prepare(Projection {
                                        layout: layout.clone(),
                                        search: search.clone(),
                                        domain: domain.clone(),
                                        first_row,
                                        scheme: colour,
                                        width,
                                        scale: window.scale_factor(),
                                        origin_x: bounds.origin.x.into(),
                                        colors: std::array::from_fn(|i| {
                                            colors.get(i).copied().unwrap_or(highlight).background
                                        }),
                                    });
                                    for slot in
                                        0..slots.min(layout.rows().saturating_sub(first_row))
                                    {
                                        let y = match orientation {
                                            Orientation::Flame => {
                                                height - (slot + 1) as f64 * ROW_HEIGHT
                                            }
                                            Orientation::Icicle => slot as f64 * ROW_HEIGHT,
                                        };
                                        let row_rect = Bounds::new(
                                            bounds.origin + point(px(0.), px(y as f32)),
                                            size(bounds.size.width, px(ROW_HEIGHT as f32 - 1.)),
                                        );
                                        if strips.paint(slot, row_rect, window, cx) {
                                            continue;
                                        }
                                        layout.visit_paint_row(
                                            first_row + slot,
                                            domain.clone(),
                                            width,
                                            search.as_deref(),
                                            |segment| {
                                                let frame =
                                                    segment.id.and_then(|id| layout.frame(id));
                                                let color = frame.map_or(0, |frame| {
                                                    palette_index(frame, colour)
                                                });
                                                let rect = Bounds::new(
                                                    bounds.origin
                                                        + point(
                                                            px(segment.x.start as f32),
                                                            px(y as f32),
                                                        ),
                                                    size(
                                                        px((segment.x.end - segment.x.start)
                                                            as f32),
                                                        px(ROW_HEIGHT as f32 - 1.),
                                                    ),
                                                );
                                                let style = if segment.highlighted {
                                                    highlight
                                                } else {
                                                    colors[color]
                                                };
                                                window.paint_quad(fill(
                                                    rect,
                                                    style.background.to_hsla(),
                                                ));
                                                if let Some(frame) =
                                                    frame.filter(|_| rect.size.width >= px(32.))
                                                {
                                                    let text = labels.text(frame.label);
                                                    let len = text.len();
                                                    let line = window.text_system().shape_line(
                                                        text,
                                                        px(11.),
                                                        &[gpui_kit::TextRun {
                                                            len,
                                                            font: gpui_kit::Font::default(),
                                                            color: style.foreground.to_hsla(),
                                                            background_color: None,
                                                            underline: None,
                                                            strikethrough: None,
                                                        }],
                                                        None,
                                                    );
                                                    window.with_content_mask(
                                                        Some(ContentMask { bounds: rect }),
                                                        |window| {
                                                            let _ = line.paint(
                                                                rect.origin + point(px(4.), px(2.)),
                                                                px(16.),
                                                                gpui_kit::TextAlign::Left,
                                                                None,
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    );
                                                }
                                            },
                                        );
                                    }
                                });
                            },
                        )
                        .size_full(),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_, _, window, _| {
                                let pointer = pointer.clone();
                                window.on_mouse_event(
                                    move |event: &gpui_kit::MouseMoveEvent, phase, _, cx| {
                                        if phase == DispatchPhase::Bubble {
                                            let _ = pointer.update(cx, |this, cx| {
                                                if this.bounds.contains(&event.position) {
                                                    this.motion(event, cx);
                                                } else {
                                                    this.drag = None;
                                                    this.set_hover(None, cx);
                                                }
                                            });
                                        }
                                    },
                                );
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .children((!self.window_tooltip).then_some(tooltip)),
            )
    }
}

/// Install a root overlay so hover can repaint independently on upstream GPUI.
pub fn init(cx: &mut App) {
    if !gpui_kit::GPUI_FAST {
        Root::register_plugin(cx, |_, _| TooltipLayer::default());
    }
}

#[derive(Default)]
struct TooltipLayer(Vec<WeakEntity<Tooltip>>);

impl RootPlugin for TooltipLayer {}

impl Render for TooltipLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.retain(|tooltip| tooltip.upgrade().is_some());
        div().absolute().size_full().children(
            self.0
                .iter()
                .filter_map(WeakEntity::upgrade)
                .map(|tooltip| tooltip.cached(StyleRefinement::default().absolute().size_full())),
        )
    }
}

struct Tooltip {
    text: Option<String>,
    anchor: Point<Pixels>,
    global: bool,
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = GuiraffeTheme::global(cx).surfaces();
        let (top, right) = if self.global {
            (self.anchor.y, window.viewport_size().width - self.anchor.x)
        } else {
            (px(6.), px(6.))
        };
        div()
            .absolute()
            .top(top)
            .right(right)
            .children(self.text.clone().map(|text| {
                div()
                    .id("flame-tooltip")
                    .role(Role::Tooltip)
                    .bg(surface.body.to_hsla())
                    .text_color(surface.text.to_hsla())
                    .p_1()
                    .child(text)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Frame, FrameSource};
    use gpui_kit::test::TestQueryExt;
    use gpui_kit::{
        Modifiers, ScrollDelta, ScrollEvent, ScrollSource, StyleRefinement, TestAppContext,
        TouchPhase,
    };
    use std::{cell::Cell, rc::Rc};

    struct Profile;
    impl FrameSource for Profile {
        fn len(&self) -> usize {
            2
        }
        fn frame(&self, index: usize) -> Frame<'_> {
            Frame {
                id: index as FrameId,
                parent: (index == 1).then_some(0),
                total: if index == 0 { 100. } else { 45. },
                self_value: 45.,
                label: if index == 0 { "root" } else { "child" },
                package: "test",
            }
        }
    }

    struct Host(Entity<FlameGraph>, Rc<Cell<usize>>);
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.1.set(self.1.get() + 1);
            div().size_full().child(
                self.0
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
        }
    }

    struct Container(Entity<Host>);
    impl Render for Container {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                self.0
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
        }
    }

    #[test]
    fn repeated_labels_reuse_storage_and_conflicts_replace_entries() {
        let mut labels = LabelCache(std::array::from_fn(|_| None));
        let first = labels.text_in_slot("worker::process_background_task", 0);
        let repeat = labels.text_in_slot("worker::process_background_task", 0);
        assert_eq!(first.as_ptr(), repeat.as_ptr());

        let other = labels.text_in_slot("render::build_accessibility_tree", 0);
        let repeat = labels.text_in_slot("render::build_accessibility_tree", 0);
        assert_eq!(other.as_ptr(), repeat.as_ptr());
        assert_eq!(labels.0.iter().flatten().count(), 1);
        labels.text(&"x".repeat(MAX_CACHED_LABEL_BYTES + 1));
        assert_eq!(labels.0.iter().flatten().count(), 1);
    }

    #[test]
    fn clamped_pan_or_zoom_leaves_domain_unchanged() -> Result<(), crate::LayoutError> {
        let layout = Arc::new(FlameLayout::new(Arc::new(Profile))?);
        let mut graph = FlameGraph::new(layout);
        assert!(!graph.set_domain(-20., 150.));
        assert_eq!(graph.domain, 0. ..100.);
        assert!(graph.set_domain(20., 20.));
        assert!(!graph.set_domain(20., 20.));
        Ok(())
    }

    struct LargeProfile {
        invalid_tail: bool,
    }

    impl FrameSource for LargeProfile {
        fn len(&self) -> usize {
            PREVIEW_FRAMES + 1
        }

        fn frame(&self, index: usize) -> Frame<'_> {
            let total = if self.invalid_tail && index == PREVIEW_FRAMES {
                -1.
            } else {
                1.
            };
            Frame {
                id: index as u64,
                parent: None,
                self_value: 0.,
                total,
                label: "frame",
                package: "test",
            }
        }
    }

    #[gpui_kit::test]
    fn deferred_source_validates_and_publishes_after_preview(cx: &mut TestAppContext) {
        for invalid_tail in [false, true] {
            let graph = cx.new(|cx| {
                let graph = FlameGraph::from_source(Arc::new(LargeProfile { invalid_tail }), cx)
                    .expect("valid preview");
                assert!(graph.is_loading());
                assert_eq!(graph.domain.end, PREVIEW_FRAMES as f64);
                graph
            });
            cx.run_until_parked();
            cx.update(|cx| {
                let graph = graph.read(cx);
                assert!(!graph.is_loading());
                if invalid_tail {
                    assert_eq!(
                        graph.load_failure(),
                        Some(LayoutError::InvalidValue(PREVIEW_FRAMES as u64))
                    );
                    assert_eq!(graph.domain.end, PREVIEW_FRAMES as f64);
                } else {
                    assert_eq!(graph.load_failure(), None);
                    assert_eq!(graph.domain.end, (PREVIEW_FRAMES + 1) as f64);
                }
            });
        }
    }

    #[gpui_kit::test]
    fn plot_pointer_zoom_pan_focus_and_reset(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::base::init(cx);
            init(cx);
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
        });
        let renders = Rc::new(Cell::new(0));
        let clicks = Rc::new(Cell::new(0));
        let graph = cx.new(|_| {
            FlameGraph::new(Arc::new(
                FlameLayout::new(Arc::new(Profile)).expect("valid fixture"),
            ))
        });
        let (_, window) = cx.add_window_view(|window, cx| {
            let clicks = clicks.clone();
            cx.subscribe(&graph, move |_, _, event: &FrameClicked, _| {
                assert_eq!(event.path, vec![0, 1]);
                clicks.set(clicks.get() + 1);
            })
            .detach();
            let host = cx.new(|_| Host(graph.clone(), renders.clone()));
            let container = cx.new(|_| Container(host));
            gpui_kit::base::Root::new(container, window, cx)
        });
        window.run_until_parked();
        window.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = graph.read_with(window, |graph, _| graph.bounds);
        let position = point(
            bounds.left() + bounds.size.width * 0.15,
            bounds.bottom() - px(30.),
        );
        let before = renders.get();
        window.simulate_mouse_move(position, None, Modifiers::default());
        window.run_until_parked();
        assert_eq!(graph.read_with(window, |graph, _| graph.hover), Some(1));
        assert_eq!(renders.get(), before, "tooltip invalidated the host");
        for control in [false, true] {
            let before = graph.read_with(window, |graph, _| graph.domain.clone());
            window.simulate_event(ScrollEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(35.))),
                modifiers: Modifiers {
                    control,
                    ..Default::default()
                },
                touch_phase: TouchPhase::Moved,
                source: ScrollSource::Pointer,
            });
            window.run_until_parked();
            let after = graph.read_with(window, |graph, _| graph.domain.clone());
            assert!(after.end - after.start < before.end - before.start);
        }
        let before = graph.read_with(window, |graph, _| graph.domain.start);
        window.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
        let end = position - point(px(22.), px(0.));
        window.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        window.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        window.run_until_parked();
        assert!(graph.read_with(window, |graph, _| graph.domain.start) > before);
        window.simulate_click(position, Modifiers::default());
        window.run_until_parked();
        assert_eq!(clicks.get(), 1);
        assert_eq!(graph.read_with(window, |graph, _| graph.focus), Some(1));
        let root = window.update(|window, _| window.find_by_label("root").bounds().center());
        window.simulate_click(root, Modifiers::default());
        window.run_until_parked();
        assert_eq!(graph.read_with(window, |graph, _| graph.focus), Some(0));
        let reset = point(bounds.left() + px(15.), bounds.top() - px(12.));
        window.simulate_click(reset, Modifiers::default());
        window.run_until_parked();
        assert_eq!(
            graph.read_with(window, |graph, _| graph.domain.clone()),
            0.0..100.0
        );

        let keyboard_focus = graph
            .read_with(window, |graph, _| graph.keyboard_focus.clone())
            .expect("plot has a focus handle");
        window.update(|window, cx| {
            let plot = window.find_by_label("Flame graph");
            assert_eq!(plot.role(), Some(Role::Canvas));
            assert_eq!(plot.description(), Some(KEYBOARD_HELP));
            window.focus(&keyboard_focus, cx);
        });
        window.simulate_keystrokes("right up enter");
        window.run_until_parked();
        assert_eq!(graph.read_with(window, |graph, _| graph.focus), Some(1));
        assert_eq!(clicks.get(), 2);
        window.update(|window, _| {
            let plot = window.find_by_label("Flame graph");
            assert!(plot.description().is_some_and(|text| text.contains("self")));
        });
        window.simulate_keystrokes("right left down home");
        window.run_until_parked();
        assert_eq!(graph.read_with(window, |graph, _| graph.focus), None);
    }
}

pub(crate) mod document;
