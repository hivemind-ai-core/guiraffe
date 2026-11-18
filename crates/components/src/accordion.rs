use std::rc::Rc;

use gpui_kit::{
    base::{Accordion as KitAccordion, AccordionPanel, AccordionTrigger},
    component::{Icon, IconName},
    prelude::*,
    *,
};
use guiraffe_core::{GuiraffeTheme, Press, PressableExt, Variant, keyed_focus};

use crate::style::{hsla, radius, surfaces};

/// A reusable body factory, invoked when its disclosure panel is mounted.
pub type DisclosureBody = Rc<dyn Fn() -> AnyElement>;

pub struct DisclosureItem {
    pub title: SharedString,
    pub body: DisclosureBody,
    pub disabled: bool,
}

impl DisclosureItem {
    pub fn new(title: impl Into<SharedString>, body: impl Fn() -> AnyElement + 'static) -> Self {
        Self {
            title: title.into(),
            body: Rc::new(body),
            disabled: false,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum AccordionVariant {
    Separated,
    #[default]
    Contained,
    Filled,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ChevronPosition {
    Start,
    #[default]
    End,
}

#[derive(Default)]
pub struct AccordionOptions {
    pub variant: AccordionVariant,
    pub chevron: ChevronPosition,
    pub multiple: bool,
    pub focusable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccordionChange(pub Vec<usize>);

/// Accordion using Kit's accordion container and GUIraffe disclosure triggers.
pub struct Accordion {
    items: Vec<DisclosureItem>,
    options: AccordionOptions,
    open: Vec<usize>,
    controlled: bool,
}

impl EventEmitter<AccordionChange> for Accordion {}

impl Accordion {
    pub fn new(
        items: Vec<DisclosureItem>,
        options: AccordionOptions,
        default_open: Vec<usize>,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| {
            let mut this = Self {
                items,
                options,
                open: Vec::new(),
                controlled: false,
            };
            this.open = this.normalize(default_open);
            this
        })
    }

    fn normalize(&self, mut open: Vec<usize>) -> Vec<usize> {
        open.retain(|&index| index < self.items.len());
        open.sort_unstable();
        open.dedup();
        if !self.options.multiple {
            open.truncate(1);
        }
        open
    }

    /// Supply the authoritative value, or pass None to resume internal state.
    pub fn set_controlled(&mut self, value: Option<Vec<usize>>, cx: &mut Context<Self>) {
        self.controlled = value.is_some();
        if let Some(value) = value {
            self.open = self.normalize(value);
        }
        cx.notify();
    }

    pub fn set_options(&mut self, options: AccordionOptions, cx: &mut Context<Self>) {
        self.options = options;
        let open = std::mem::take(&mut self.open);
        self.open = self.normalize(open);
        cx.notify();
    }

    pub fn open(&self) -> &[usize] {
        &self.open
    }

    fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut next = self.open.clone();
        match next.binary_search(&index) {
            Ok(position) => {
                next.remove(position);
            }
            Err(position) if self.options.multiple => next.insert(position, index),
            Err(_) => next = vec![index],
        }
        if !self.controlled {
            self.open.clone_from(&next);
            cx.notify();
        }
        cx.emit(AccordionChange(next));
    }
}

impl Render for Accordion {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = surfaces(cx);
        let radius = radius(None, cx);
        let separated = self.options.variant == AccordionVariant::Separated;
        let colors = GuiraffeTheme::global(cx).press_colors(Variant::Subtle, None);
        let id = cx.entity_id();
        KitAccordion::new(("accordion", id.as_u64() as usize))
            .flex()
            .flex_col()
            .w_full()
            .when(separated, |el| el.gap_2())
            .when(!separated, |el| {
                el.border_1()
                    .border_color(hsla(surface.border))
                    .rounded(radius)
            })
            .children(self.items.iter().enumerate().map(|(index, item)| {
                let open = self.open.contains(&index);
                let trigger_id = ElementId::from((
                    SharedString::from(format!("accordion-{}", id.as_u64() as usize)),
                    index,
                ));
                let focus = keyed_focus(trigger_id.clone(), window, cx);
                let press = Press::new(colors)
                    .label(item.title.clone())
                    .disabled(item.disabled)
                    .focusable(self.options.focusable)
                    .radius(radius);
                let chevron = if open {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                };
                let trigger = AccordionTrigger::new(trigger_id)
                    .open(open)
                    .disabled(item.disabled)
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(radius)
                    .when(self.options.chevron == ChevronPosition::Start, |el| {
                        el.child(Icon::new(chevron.clone()))
                    })
                    .child(div().flex_1().child(item.title.clone()))
                    .when(self.options.chevron == ChevronPosition::End, |el| {
                        el.child(Icon::new(chevron))
                    })
                    .press(&press, &focus, window)
                    .on_press(
                        item.disabled,
                        cx.listener(move |this, _, _, cx| this.toggle(index, cx)),
                    );
                div()
                    .flex()
                    .flex_col()
                    .when(separated, |el| {
                        el.border_1()
                            .border_color(hsla(surface.border))
                            .rounded(radius)
                    })
                    .when(!separated && index != 0, |el| {
                        el.border_t_1().border_color(hsla(surface.border))
                    })
                    .when(self.options.variant == AccordionVariant::Filled, |el| {
                        el.bg(hsla(surface.track))
                    })
                    .child(trigger)
                    .when(open, |el| {
                        el.child(
                            AccordionPanel::new()
                                .id(("accordion-panel", index))
                                .open(true)
                                .px_3()
                                .pb_3()
                                .child((item.body)()),
                        )
                    })
            }))
    }
}
