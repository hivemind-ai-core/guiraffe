use std::{f32::consts::PI, time::Duration};

use gpui_kit::{
    component::tooltip::Tooltip,
    prelude::*,
    *,
};
use gpui_kit::assets::IconName;
use guiraffe_core::{A11yDisabledExt, GuiraffeTheme, Press, PressableExt, Variant, keyed_focus};
use guiraffe_motion::{Animated, MotionPrefs, Spring, Timing};

use crate::style::radius;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DialLayout {
    Vertical {
        spacing: f32,
    },
    Horizontal {
        spacing: f32,
    },
    Radial {
        radius: f32,
        start_degrees: f32,
        arc: DialArc,
        clockwise: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialArc {
    Quarter,
    Half,
}

impl DialLayout {
    /// Positions relative to the primary button, with positive Y down.
    pub fn offsets(self, count: usize) -> Vec<(f32, f32)> {
        (0..count)
            .map(|index| match self {
                Self::Vertical { spacing } => (0., -(index as f32 + 1.) * spacing),
                Self::Horizontal { spacing } => ((index as f32 + 1.) * spacing, 0.),
                Self::Radial {
                    radius,
                    start_degrees,
                    arc,
                    clockwise,
                } => {
                    let span = match arc {
                        DialArc::Quarter => PI / 2.,
                        DialArc::Half => PI,
                    };
                    let direction = if clockwise { 1. } else { -1. };
                    let angle = start_degrees.to_radians()
                        + direction * span * index as f32 / count.saturating_sub(1).max(1) as f32;
                    (radius * angle.cos(), radius * angle.sin())
                }
            })
            .collect()
    }
}

/// Delay for one bubble. Cap the total delay for long menus.
pub fn stagger_delay(index: usize) -> Duration {
    Duration::from_millis(24 * index.min(8) as u64)
}

pub struct DialAction {
    pub icon: IconName,
    pub label: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedDialEvent {
    Open(bool),
    Action(usize),
}

pub struct SpeedDial {
    actions: Vec<DialAction>,
    positions: Vec<(f32, f32)>,
    springs: Vec<Animated<f32>>,
    open: bool,
    focusable: Option<bool>,
    primary: IconName,
    label: SharedString,
}

impl EventEmitter<SpeedDialEvent> for SpeedDial {}

impl SpeedDial {
    pub fn new(
        primary: IconName,
        label: impl Into<SharedString>,
        actions: Vec<DialAction>,
        layout: DialLayout,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| Self {
            primary,
            label: label.into(),
            positions: layout.offsets(actions.len()),
            springs: vec![Animated::new(0., Timing::Spring(Spring::SNAPPY)); actions.len()],
            actions,
            open: false,
            focusable: None,
        })
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_layout(&mut self, layout: DialLayout, cx: &mut Context<Self>) {
        self.positions = layout.offsets(self.actions.len());
        cx.notify();
    }

    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = focusable;
        cx.notify();
    }

    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open == open {
            return;
        }
        self.open = open;
        let prefs = MotionPrefs::global(cx);
        let now = cx.background_executor().now();
        for (index, spring) in self.springs.iter_mut().enumerate() {
            let target = if open { 1. } else { 0. };
            if prefs.reduced {
                spring.jump(target);
            } else if open {
                let (current, _) = spring.value_at(now, &prefs);
                spring.jump(current);
                spring.set_at(target, now + stagger_delay(index), &prefs);
            } else {
                spring.set_at(target, now, &prefs);
            }
        }
        cx.notify();
        cx.emit(SpeedDialEvent::Open(open));
    }
}

impl Render for SpeedDial {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let corner = radius(None, cx);
        let theme = GuiraffeTheme::global(cx);
        let primary_colors = theme.press_colors(Variant::Filled, None);
        let action_colors = theme.press_colors(Variant::Default, None);
        let group = SharedString::from(format!("speed-dial-{}", cx.entity_id().as_u64()));
        let id = |index| ElementId::from((group.clone(), index));
        let primary_focus = keyed_focus(id(0), window, cx);
        let keyboard = guiraffe_core::tab_stop(self.focusable);
        let mut content = div().id(group.clone()).relative().size(px(40.));
        let mut handles = vec![primary_focus.clone()];
        let mut positions = Vec::with_capacity(self.springs.len());
        for (index, ((&(x, y), spring), action)) in self
            .positions
            .iter()
            .zip(&mut self.springs)
            .zip(&self.actions)
            .enumerate()
        {
            let p = spring.sample(window, cx).clamp(0., 1.);
            if p == 0. {
                continue;
            }
            positions.push(point(px(x * p), px(y * p)));
            let action_id = id(index + 1);
            let focus = keyed_focus(action_id.clone(), window, cx);
            if self.open {
                handles.push(focus.clone());
            }
            let label = action.label.clone();
            let tooltip = label.clone();
            let press = Press::new(action_colors)
                .label(label)
                .radius(corner)
                .disabled(!self.open)
                .focusable(Some(self.open && keyboard));
            let return_focus = primary_focus.clone();
            let bubble = div()
                .id(action_id)
                .a11y_disabled(!self.open)
                .absolute()
                .size(px(40.))
                .left(px(x * p))
                .top(px(y * p))
                .opacity(p)
                .flex()
                .items_center()
                .justify_center()
                .rounded(corner)
                .border_1()
                .child(action.icon)
                .press(&press, &focus, window)
                .when(self.open, |el| {
                    el.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                })
                .on_press(
                    !self.open,
                    cx.listener(move |this, _, window, cx| {
                        if focus.is_focused(window) {
                            return_focus.focus(window, cx);
                        }
                        cx.emit(SpeedDialEvent::Action(index));
                        this.set_open(false, cx);
                    }),
                );
            content = content.child(bubble);
        }
        let press = Press::new(primary_colors)
            .label(self.label.clone())
            .radius(corner)
            .focusable(self.focusable);
        let focus_on_escape = primary_focus.clone();
        let entity = cx.entity();
        let primary = div()
            .id(id(0))
            .aria_expanded(self.open)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded(corner)
            .border_1()
            .child(if self.open {
                IconName::X
            } else {
                self.primary
            })
            .press(&press, &primary_focus, window)
            .on_press(
                false,
                cx.listener(|this, _, _, cx| this.set_open(!this.open, cx)),
            );
        // GPUI dispatches outside presses for the primary box even when the dial is open.
        // Check bubble hit boxes before dismissing so their press can complete.
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measured = bounds.clone();
        content
            .child(primary)
            .child(
                canvas(move |b, _, _| measured.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .when(self.open, |el| {
                el.on_mouse_down_out(move |event, _, cx| {
                    let anchor = bounds.get();
                    if !positions.iter().any(|offset| {
                        Bounds {
                            origin: anchor.origin + *offset,
                            size: anchor.size,
                        }
                        .contains(&event.position)
                    }) {
                        entity.update(cx, |this, cx| this.set_open(false, cx));
                    }
                })
            })
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if !keyboard {
                    return;
                }
                let key = event.keystroke.key.as_str();
                if key == "escape" && this.open {
                    focus_on_escape.focus(window, cx);
                    this.set_open(false, cx);
                } else if this.open && matches!(key, "up" | "down" | "left" | "right") {
                    let current = handles
                        .iter()
                        .position(|focus| focus.is_focused(window))
                        .unwrap_or(0);
                    let step = if matches!(key, "left" | "up") {
                        handles.len() - 1
                    } else {
                        1
                    };
                    handles[(current + step) % handles.len()].focus(window, cx);
                } else {
                    return;
                }
                cx.stop_propagation();
            })
    }
}
