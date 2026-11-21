use crate::disclosure_motion;
use gpui_kit::{
    component::{Icon, IconName},
    prelude::*,
    *,
};
use guiraffe_core::{A11yDisabledExt, GuiraffeTheme, Press, PressableExt, Size, keyed_focus};
use guiraffe_motion::{Animated, Spring, Timing};

use crate::{
    DisclosureItem,
    style::{hsla, radius, shadows, surfaces},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpandingListChange(pub Option<usize>);

/// A list whose open item rises out of the column. The list reserves room for
/// one expanded card so animation moves the list items without moving its host's siblings.
pub struct ExpandingList {
    items: Vec<DisclosureItem>,
    open: Option<usize>,
    progress: Vec<Animated<f32>>,
    body_height: Pixels,
    focusable: Option<bool>,
    /// The element id prefix of the triggers: it never changes, so it is built
    /// once instead of for every trigger of every frame.
    trigger_prefix: SharedString,
}

impl EventEmitter<ExpandingListChange> for ExpandingList {}

impl ExpandingList {
    /// The body height is reserved during expansion. Overflowing body content scrolls.
    pub fn new(items: Vec<DisclosureItem>, body_height: Pixels, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            progress: vec![Animated::new(0., Timing::Spring(Spring::SNAPPY)); items.len()],
            items,
            open: None,
            body_height: body_height.max(px(0.)),
            focusable: None,
            trigger_prefix: format!("expanding-{}", cx.entity_id().as_u64()).into(),
        })
    }

    pub fn open(&self) -> Option<usize> {
        self.open
    }

    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = focusable;
        cx.notify();
    }

    pub fn set_open(&mut self, open: Option<usize>, cx: &mut Context<Self>) {
        if open == self.open
            || open.is_some_and(|i| self.items.get(i).is_none_or(|item| item.disabled))
        {
            return;
        }
        if let Some(previous) = self.open {
            self.progress[previous].set(0., cx);
        }
        if let Some(next) = open {
            self.progress[next].set(1., cx);
        }
        self.open = open;
        cx.notify();
        cx.emit(ExpandingListChange(open));
    }
}

impl Render for ExpandingList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut needs_frame = false;
        let colors = disclosure_motion::trigger_colors(GuiraffeTheme::global(cx));
        let surface = surfaces(cx);
        let corner = radius(None, cx);
        let id = cx.entity_id().as_u64();
        let trigger_prefix = self.trigger_prefix.clone();
        let row_height = px(40.);
        let vertical_gap = px(8.);
        let reserved = if self.items.is_empty() {
            px(0.)
        } else {
            (row_height + px(2.)) * self.items.len() as f32 + self.body_height + vertical_gap * 2.
        };
        let element = div()
            .id(("expanding-list", id as usize))
            .w_full()
            .h(reserved)
            .flex()
            .flex_col()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.open.is_some() {
                    this.set_open(None, cx);
                    cx.stop_propagation();
                }
            }))
            .children(self.items.iter().zip(&mut self.progress).enumerate().map(
                |(index, (item, progress))| {
                    let p =
                        disclosure_motion::progress(progress, &mut needs_frame, cx).clamp(0., 1.);
                    let open = self.open == Some(index);
                    let trigger = ElementId::from((trigger_prefix.clone(), index));
                    let focus = keyed_focus(trigger.clone(), window, cx);
                    let press = Press::new(colors)
                        .label(item.title.clone())
                        .focusable(self.focusable)
                        .disabled(item.disabled)
                        .radius(corner);
                    let gap = vertical_gap * p;
                    div().flex().flex_col().flex_shrink_0().py(gap).child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .rounded(corner)
                            .border_1()
                            .border_color(hsla(surface.border))
                            .bg(hsla(surface.surface))
                            .when(p > 0., |el| el.mx(px(6. * p)).shadow(shadows(Size::Sm, cx)))
                            .child(
                                div()
                                    .id(trigger)
                                    .debug_selector(move || format!("expanding-trigger-{index}"))
                                    .a11y_disabled(item.disabled)
                                    .test_support()
                                    .aria_expanded(open)
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_3()
                                    .h(row_height)
                                    .rounded(corner)
                                    .child(item.title.clone())
                                    .child(Icon::new(if open {
                                        IconName::ChevronUp
                                    } else {
                                        IconName::ChevronDown
                                    }))
                                    .press(&press, &focus, window)
                                    .on_press(
                                        item.disabled,
                                        cx.listener(move |this, _, _, cx| {
                                            this.set_open(
                                                (this.open != Some(index)).then_some(index),
                                                cx,
                                            );
                                        }),
                                    ),
                            )
                            .when(p > 0., |el| {
                                el.child(
                                    div()
                                        .relative()
                                        .h(self.body_height * p)
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .id(("expanded-body", index))
                                                .h(self.body_height)
                                                .px_3()
                                                .pb_3()
                                                .overflow_y_scroll()
                                                .child((item.body)()),
                                        ),
                                )
                            }),
                    )
                },
            ));
        disclosure_motion::schedule(needs_frame, cx.entity_id(), window);
        element
    }
}
