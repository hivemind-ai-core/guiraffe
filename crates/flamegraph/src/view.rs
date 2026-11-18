use crate::{FlameLayout, FrameClicked, FrameId, Search};
use gpui_kit::{
    Bounds, Context, EventEmitter, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Pixels, Point, Render, Role, SharedString, StatefulInteractiveElement, Styled, Window, canvas,
    component::button::Button, div, fill, point, px, size,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Variant};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    sync::Arc,
};

const ROW_HEIGHT: f64 = 21.;

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

pub struct FlameGraph {
    layout: Arc<FlameLayout>,
    domain: Range<f64>,
    focus: Option<FrameId>,
    hover: Option<FrameId>,
    drag: Option<(Point<Pixels>, bool)>,
    search: Option<Search>,
    orientation: Orientation,
    colour: ColourScheme,
    bounds: Bounds<Pixels>,
    first_row: usize,
}

impl EventEmitter<FrameClicked> for FlameGraph {}

impl FlameGraph {
    pub fn new(layout: Arc<FlameLayout>) -> Self {
        Self {
            domain: layout.domain(),
            layout,
            focus: None,
            hover: None,
            drag: None,
            search: None,
            orientation: Orientation::Flame,
            colour: ColourScheme::Name,
            bounds: Bounds::default(),
            first_row: 0,
        }
    }

    pub fn set_search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.search = self.layout.search(query);
        cx.notify();
    }

    pub fn set_orientation(&mut self, orientation: Orientation, cx: &mut Context<Self>) {
        self.orientation = orientation;
        self.hover = None;
        cx.notify();
    }

    pub fn set_colour_scheme(&mut self, colour: ColourScheme, cx: &mut Context<Self>) {
        self.colour = colour;
        cx.notify();
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.domain = self.layout.domain();
        self.first_row = 0;
        self.hover = None;
        self.focus = None;
        cx.notify();
    }

    fn row_y(&self, slot: usize) -> f64 {
        match self.orientation {
            Orientation::Flame => {
                f64::from(f32::from(self.bounds.size.height)) - (slot + 1) as f64 * ROW_HEIGHT
            }
            Orientation::Icicle => slot as f64 * ROW_HEIGHT,
        }
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

    fn set_domain(&mut self, start: f64, span: f64) {
        let total = self.layout.domain().end;
        if total > 0. && span.is_finite() && start.is_finite() {
            let span = span.clamp(total * 1e-12, total);
            let start = start.clamp(0., total - span);
            self.domain = start..start + span;
        }
    }

    fn focus_frame(&mut self, id: FrameId) {
        if let (Some(domain), Some(event)) = (self.layout.focus(id), self.layout.click(id)) {
            self.domain = domain;
            self.first_row = event.path.len() - 1;
            self.focus = Some(id);
            self.hover = None;
        }
    }
}

impl Render for FlameGraph {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let surface = theme.surfaces();
        let palette = [
            ColorName::Blue,
            ColorName::Cyan,
            ColorName::Teal,
            ColorName::Orange,
            ColorName::Grape,
            ColorName::Pink,
        ]
        .map(|name| {
            theme
                .variant(Some(name), Variant::Filled)
                .background
                .to_hsla()
        });
        let highlight = theme
            .variant(Some(theme.primary), Variant::Filled)
            .border
            .to_hsla();
        let breadcrumb = self.focus.and_then(|id| self.layout.click(id));
        let tooltip = self
            .hover
            .and_then(|id| self.layout.frame(id))
            .map(|frame| {
                format!(
                    "{} · self {:.3} · total {:.3}",
                    frame.label, frame.self_value, frame.total
                )
            });
        let prepare = cx.entity().downgrade();
        let paint = prepare.clone();
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
                            .label("Reset")
                            .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                    )
                    .children(
                        breadcrumb
                            .into_iter()
                            .flat_map(|event| event.path)
                            .filter_map(|id| {
                                let label =
                                    SharedString::from(self.layout.frame(id)?.label.to_owned());
                                Some(Button::new(("frame", id as usize)).label(label).on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.focus_frame(id);
                                        cx.notify();
                                    }),
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .id("flame-plot")
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .role(Role::Canvas)
                    .aria_label("Flame graph")
                    .bg(surface.body.to_hsla())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, _| {
                            this.drag = Some((event.position, false))
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(|this, event: &gpui_kit::MouseMoveEvent, _, cx| {
                            if let Some((previous, moved)) = this.drag {
                                let dx = f64::from(f32::from(event.position.x - previous.x));
                                if this.width() > 0. && (moved || dx.abs() >= 3.) {
                                    let span = this.domain.end - this.domain.start;
                                    this.set_domain(
                                        this.domain.start - dx * span / this.width(),
                                        span,
                                    );
                                    this.drag = Some((event.position, true));
                                    cx.notify();
                                }
                            }
                            let hover = this.hit(event.position);
                            if hover != this.hover {
                                this.hover = hover;
                                cx.notify();
                            }
                        }),
                    )
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !hovered {
                            this.drag = None;
                            this.hover = None;
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui_kit::MouseUpEvent, _, cx| {
                            if let Some((_, false)) = this.drag.take()
                                && let Some(id) = this.hit(event.position)
                                && let Some(clicked) = this.layout.click(id)
                            {
                                this.focus_frame(id);
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
                        this.set_domain(
                            this.domain.start + anchor * (span - next)
                                - f64::from(f32::from(delta.x)) * span / this.width(),
                            next,
                        );
                        this.hover = this.hit(event.position);
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let _ = prepare.update(cx, |this, _| this.bounds = bounds);
                            },
                            move |bounds, _, window, cx| {
                                let Some(entity) = paint.upgrade() else {
                                    return;
                                };
                                let this = entity.read(cx);
                                let slots = (f64::from(f32::from(bounds.size.height)) / ROW_HEIGHT)
                                    .ceil() as usize;
                                for slot in
                                    0..slots.min(this.layout.rows().saturating_sub(this.first_row))
                                {
                                    this.layout.visit_row(
                                        this.first_row + slot,
                                        this.domain.clone(),
                                        this.width(),
                                        this.search.as_ref(),
                                        |segment| {
                                            let color = segment
                                                .id
                                                .and_then(|id| this.layout.frame(id))
                                                .map_or(0, |frame| match this.colour {
                                                    ColourScheme::Heat => {
                                                        ((frame.self_value / frame.total)
                                                            * (palette.len() - 1) as f64)
                                                            as usize
                                                    }
                                                    scheme => {
                                                        let mut hash = DefaultHasher::new();
                                                        if scheme == ColourScheme::Package {
                                                            frame.package
                                                        } else {
                                                            frame.label
                                                        }
                                                        .hash(&mut hash);
                                                        hash.finish() as usize % palette.len()
                                                    }
                                                });
                                            let rect = Bounds::new(
                                                bounds.origin
                                                    + point(
                                                        px(segment.x.start as f32),
                                                        px(this.row_y(slot) as f32),
                                                    ),
                                                size(
                                                    px((segment.x.end - segment.x.start) as f32),
                                                    px(ROW_HEIGHT as f32 - 1.),
                                                ),
                                            );
                                            window.paint_quad(fill(
                                                rect,
                                                if segment.highlighted {
                                                    highlight
                                                } else {
                                                    palette[color]
                                                },
                                            ));
                                        },
                                    );
                                }
                            },
                        )
                        .size_full(),
                    )
                    .children(tooltip.map(|text| {
                        div()
                            .id("flame-tooltip")
                            .absolute()
                            .top(px(6.))
                            .right(px(6.))
                            .role(Role::Tooltip)
                            .bg(surface.body.to_hsla())
                            .p_1()
                            .child(text)
                    })),
            )
    }
}
