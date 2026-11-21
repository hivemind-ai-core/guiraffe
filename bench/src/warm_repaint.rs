//! Warm-repaint allocation audit: a widget tree mounted headlessly, drawn until
//! it is stable, then counted.
//!
//! These are the crates every frame goes through - components, inputs, layout
//! and overlays - drawn again and again without changing. What they allocate
//! is what a warm repaint costs, so the suites are the ones the allocator gate
//! measures: a warm repaint of an unchanged view must not grow.

use anyhow::{Context as _, Result};
use gpui_kit::{
    AppContext, Context, Entity, HeadlessAppContext, IntoElement, ParentElement, Render, Styled,
    Window, WindowHandle, div, px, size,
};
use std::sync::Arc;

/// The widget families the audit covers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Components,
    Inputs,
    Layout,
    Overlays,
}

/// One mounted fixture: the widgets it holds are built once and drawn again.
struct Host {
    family: Family,
    accordion: Entity<guiraffe_components::Accordion>,
    segmented: Entity<guiraffe_components::SegmentedControl>,
    copy: Entity<guiraffe_components::CopyButton>,
    chips: Entity<guiraffe_inputs::ChipGroup>,
    tags: Entity<guiraffe_inputs::TagsInput>,
    range: Entity<guiraffe_inputs::RangeSlider>,
    dropzone: Entity<guiraffe_inputs::Dropzone>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self.family {
            Family::Components => {
                let mut card = guiraffe_components::Card::new();
                card.extend([
                    guiraffe_components::Title::new("Warm repaint")
                        .order(3)
                        .into_any_element(),
                    guiraffe_components::Text::new("A card that never changes.").into_any_element(),
                ]);
                div()
                    .size_full()
                    .child(card)
                    .child(self.accordion.clone())
                    .child(self.segmented.clone())
                    .child(self.copy.clone())
                    .into_any_element()
            }
            Family::Inputs => div()
                .size_full()
                .child(self.chips.clone())
                .child(self.tags.clone())
                .child(self.range.clone())
                .child(self.dropzone.clone())
                .into_any_element(),
            Family::Layout => {
                use guiraffe_layout::{Col, Container, Divider, Flex, Grid, Group, Stack};
                div()
                    .size_full()
                    .child(
                        Stack::new()
                            .gap(guiraffe_core::Size::Md)
                            .child(Group::new().child(guiraffe_components::Text::new("Group")))
                            .child(
                                Grid::new()
                                    .child(
                                        Col::span(6).child(guiraffe_components::Text::new("left")),
                                    )
                                    .child(
                                        Col::span(6).child(guiraffe_components::Text::new("right")),
                                    ),
                            )
                            .child(Flex::new().child(guiraffe_components::Text::new("flex")))
                            .child(
                                Container::new().child(guiraffe_components::Text::new("container")),
                            )
                            .child(Divider::new()),
                    )
                    .into_any_element()
            }
            Family::Overlays => div()
                .size_full()
                .child(guiraffe_overlays::LoadingOverlay::new())
                .into_any_element(),
        }
    }
}

/// A mounted fixture, ready to be drawn.
pub(crate) struct Warm {
    app: HeadlessAppContext,
    window: WindowHandle<Host>,
}

impl Warm {
    pub(crate) fn new(family: Family) -> Result<Self> {
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut app =
            HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), || {
                Ok(None)
            });
        let mut host = None;
        let window = app.open_window(size(px(1200.), px(800.)), |window, cx| {
            guiraffe_components::init(cx);
            guiraffe_inputs::init(cx);
            guiraffe_overlays::init(cx);
            let accordion = guiraffe_components::Accordion::new(
                vec![guiraffe_components::DisclosureItem::new("Item", || {
                    guiraffe_components::Text::new("Body").into_any_element()
                })],
                guiraffe_components::AccordionOptions::default(),
                vec![0],
                cx,
            );
            let segmented = guiraffe_components::SegmentedControl::new(["One", "Two", "Three"], cx);
            let copy = guiraffe_components::CopyButton::new(cx);
            copy.update(cx, |copy, cx| copy.set_value("warm repaint", cx));
            let chips = guiraffe_inputs::ChipGroup::new(["Rust", "Go", "Zig"], true, cx);
            let tags = guiraffe_inputs::TagsInput::new(
                guiraffe_inputs::TagRules::default(),
                "tags",
                window,
                cx,
            );
            let range = guiraffe_inputs::RangeSlider::new(
                guiraffe_inputs::Range2::new(0., 100., 1., 10., 90.),
                cx,
            );
            let dropzone = guiraffe_inputs::Dropzone::new(vec!["text/plain"], "Drop files", cx);
            let control = cx.new(|_| Host {
                family,
                accordion,
                segmented,
                copy,
                chips,
                tags,
                range,
                dropzone,
            });
            host = Some(control.clone());
            control
        })?;
        host.context("warm repaint fixture was not mounted")?;
        let mut fixture = Self { app, window };
        for _ in 0..4 {
            fixture.draw()?;
        }
        Ok(fixture)
    }

    pub(crate) fn draw(&mut self) -> Result<()> {
        self.app
            .update_window(self.window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
            })?;
        self.app.run_until_parked();
        Ok(())
    }
}

/// One suite per family: draw the fixture once.
macro_rules! warm_suite {
    ($name:ident, $family:ident) => {
        /// Draw the family's fixture once.
        pub fn $name() {
            use crate::warm_repaint::{Family, Warm};
            thread_local! {
                static FIXTURE: std::cell::RefCell<anyhow::Result<Warm>> =
                    std::cell::RefCell::new(Warm::new(Family::$family));
            }
            FIXTURE.with_borrow_mut(|fixture| {
                let result = match fixture {
                    Ok(fixture) => fixture.draw(),
                    Err(error) => panic!("warm repaint setup failed: {error:#}"),
                };
                if let Err(error) = result {
                    panic!("warm repaint draw failed: {error:#}");
                }
            });
        }
    };
}

warm_suite!(warm_components, Components);
warm_suite!(warm_inputs, Inputs);
warm_suite!(warm_layout, Layout);
warm_suite!(warm_overlays, Overlays);
