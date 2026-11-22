//! guiraffe-motion: opt-in layout animation (spec/29).

use std::rc::Rc;

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    component::{Sizable, button::Button, checkbox::Checkbox},
    div, px,
};
use guiraffe_components::Text;
use guiraffe_core::{GuiraffeTheme, Size};
use guiraffe_layout::Stack;
use guiraffe_motion::{
    AnimatedList, AnimatedSize, KeyedPresence, MotionPrefs, SizeAxis, Transition, TransitionKind,
    reveal,
};
use guiraffe_tree::{
    Children, NodeId, Property, PropertyGrid, TreeData, TreeSource, TreeView, Value,
};

use super::{column, example, tile};
use crate::catalog::Entry;

pub fn entries() -> [Entry; 1] {
    [Entry {
        krate: "motion",
        name: "Layout animation",
        summary: "Opt-in: list add/remove/reorder, panel collapse and PropertyGrid groups interpolate size and position.",
        covers: &[],
        demo: layout_animation,
    }]
}

const NAMES: [&str; 8] = [
    "Inbox", "Drafts", "Sent", "Archive", "Spam", "Trash", "Starred", "Notes",
];

struct LayoutDemo {
    animate: bool,
    next: u64,
    items: Vec<u64>,
    presence: KeyedPresence<u64>,
    panel: bool,
    details: bool,
    grid: Entity<PropertyGrid>,
}

fn layout_animation(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|cx| {
        let grid = cx.new(|cx| {
            // Open the groups before enabling animation so the first frame is settled.
            let mut grid = TreeView::new(Rc::new(Props), cx);
            grid.set_expanded(0, true, cx);
            grid.set_expanded(10, true, cx);
            grid.animate_layout(Some(true))
        });
        let items = vec![0, 1, 2, 3];
        LayoutDemo {
            animate: true,
            next: 4,
            presence: KeyedPresence::new(items.clone(), transition(cx)),
            items,
            panel: true,
            details: false,
            grid,
        }
    })
    .into()
}

fn transition(cx: &App) -> Transition {
    Transition::themed(TransitionKind::Fade, &MotionPrefs::global(cx).tokens)
}

impl LayoutDemo {
    /// Preferences for this demo: with the option off, presence changes are instant.
    fn prefs(&self, cx: &App) -> MotionPrefs {
        let mut prefs = MotionPrefs::global(cx);
        prefs.reduced |= !self.animate;
        prefs
    }

    fn set_items(&mut self, items: Vec<u64>, cx: &mut Context<Self>) {
        let prefs = self.prefs(cx);
        let now = cx.background_executor().now();
        self.items = items;
        self.presence.sync(&self.items, now, &prefs);
        cx.notify();
    }
}

impl Render for LayoutDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self.prefs(cx);
        if self.presence.frame(cx.background_executor().now(), &prefs) {
            let entity = cx.entity_id();
            window.on_next_frame(move |_, cx| cx.notify(entity));
        }
        let animate = self.animate;
        let border = GuiraffeTheme::global(cx).surfaces().border.to_hsla();
        let controls = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(
                Checkbox::new("layout-animate")
                    .label("Animate layout")
                    .checked(animate)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.animate = *on;
                        let on = Some(*on);
                        this.grid.update(cx, |g, _| g.set_animate_layout(on));
                        cx.notify();
                    })),
            )
            .child(
                Button::new("layout-add")
                    .label("Add")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let mut items = this.items.clone();
                        items.insert(items.len().min(1), this.next);
                        this.next += 1;
                        this.set_items(items, cx);
                    })),
            )
            .child(
                Button::new("layout-remove")
                    .label("Remove")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let mut items = this.items.clone();
                        if !items.is_empty() {
                            items.remove(items.len() / 2);
                        }
                        this.set_items(items, cx);
                    })),
            )
            .child(
                Button::new("layout-reorder")
                    .label("Reverse")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let mut items = this.items.clone();
                        items.reverse();
                        this.set_items(items, cx);
                    })),
            )
            .child(
                Button::new("layout-panel")
                    .label("Toggle panel")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.panel = !this.panel;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("layout-details")
                    .label("Toggle details")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.details = !this.details;
                        cx.notify();
                    })),
            );

        let rows = self.presence.iter().map(|(&key, visibility)| {
            let name = NAMES[(key as usize) % NAMES.len()];
            (
                key,
                reveal(
                    ("layout-row", key),
                    visibility,
                    tile(format!("{name} #{key}"), cx).w_full(),
                ),
            )
        });
        let list = AnimatedSize::new(
            "layout-list-size",
            AnimatedList::new("layout-list")
                .gap(px(6.))
                .enabled(animate)
                .children(rows),
        )
        .enabled(animate);

        let panel = AnimatedSize::new(
            "layout-panel",
            if self.panel {
                div()
                    .w(px(110.))
                    .h_full()
                    .p(px(8.))
                    .border_r_1()
                    .border_color(border)
                    .child(Text::new("Side panel").size(Size::Sm))
            } else {
                div()
            },
        )
        .axis(SizeAxis::Horizontal)
        .enabled(animate);
        let shell = div()
            .flex()
            .h(px(96.))
            .w_full()
            .border_1()
            .border_color(border)
            .child(div().flex_none().h_full().child(panel))
            .child(
                div()
                    .flex_1()
                    .p(px(8.))
                    .child(Text::new("Main content").size(Size::Sm)),
            );

        let details = Stack::new()
            .gap_px(6.)
            .child(tile("Summary", cx))
            .children(
                self.details
                    .then(|| [tile("Detail one", cx), tile("Detail two", cx)])
                    .into_iter()
                    .flatten(),
            )
            .animate_layout_with("layout-details", Some(animate));

        // Three side-by-side columns so every example fits the 1100x760 showcase frame.
        let col = |width: f32| {
            div()
                .flex()
                .flex_col()
                .gap(px(16.))
                .w(px(width))
                .flex_none()
        };
        column([
            example("Options", controls),
            div()
                .flex()
                .items_start()
                .gap(px(16.))
                .child(col(230.).child(example("Keyed list (AnimatedList + KeyedPresence)", list)))
                .child(
                    col(270.)
                        .child(example("Collapsing details (Stack)", details))
                        .child(example("Panel collapse (AnimatedSize)", shell)),
                )
                .child(col(280.).child(example("PropertyGrid group collapse", self.grid.clone())))
                .into_any_element(),
        ])
    }
}

/// Two groups of properties for the PropertyGrid example.
struct Props;

static GROUP_A: [NodeId; 3] = [1, 2, 3];
static GROUP_B: [NodeId; 2] = [11, 12];
static VALUES: [Value; 5] = [
    Value::Bool(true),
    Value::Number(12.),
    Value::Bool(false),
    Value::Number(4.),
    Value::Bool(true),
];

impl TreeSource for Props {
    fn roots(&self) -> &[NodeId] {
        &[0, 10]
    }
    fn children(&self, id: NodeId) -> Children<'_> {
        match id {
            0 => Children::Ready(&GROUP_A),
            10 => Children::Ready(&GROUP_B),
            _ => Children::Ready(&[]),
        }
    }
    fn label(&self, id: NodeId) -> &str {
        match id {
            0 => "Appearance",
            1 => "Visible",
            2 => "Opacity",
            3 => "Locked",
            10 => "Layout",
            11 => "Columns",
            12 => "Wrap",
            _ => "",
        }
    }
}

impl TreeData for Props {
    fn property(&self, id: NodeId) -> Option<Property<'_>> {
        let value = match id {
            1..=3 => VALUES.get(id as usize - 1)?,
            11 | 12 => VALUES.get(id as usize - 8)?,
            _ => return None,
        };
        Some(Property {
            value,
            default: value,
            choices: &[],
            read_only: false,
        })
    }
}
