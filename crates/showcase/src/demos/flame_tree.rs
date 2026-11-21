use crate::{
    catalog::Entry,
    events::EventLog,
    knobs::{Knob, playground_with},
};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Styled,
    Window, assets::IconName, component::Icon, div, px,
};
use guiraffe_flamegraph::{ColourScheme, FlameGraph, Frame, FrameSource, Orientation};
use guiraffe_tree::{
    Children, NodeId, Property, PropertyOptions, RowDensity, RowStyle, TreeData, TreeEvent,
    TreeSource, TreeView, Value,
};
use std::{cell::Cell, collections::BTreeMap, rc::Rc, sync::Arc};

pub fn entries() -> [Entry; 3] {
    [
        Entry {
            krate: "flamegraph",
            name: "Flame graph",
            summary: "Inspect a profile. Scroll to zoom, drag to pan, or click to focus on a stack.",
            covers: &["FlameGraph"],
            demo: flame,
        },
        Entry {
            krate: "tree",
            name: "Tree list",
            summary: "Expand a branch, select rows, or drag a row to reorder siblings.",
            covers: &["TreeView"],
            demo: tree::<false>,
        },
        Entry {
            krate: "tree",
            name: "Property grid",
            summary: "Edit settings inline. Press Enter to commit text, numbers, and hex colours.",
            covers: &["TreeView"],
            demo: tree::<true>,
        },
    ]
}

struct Profile;
impl FrameSource for Profile {
    fn len(&self) -> usize {
        9
    }
    fn frame(&self, index: usize) -> Frame<'_> {
        let (parent, own, total, label, package) = [
            (None, 3., 100., "application", "app"),
            (Some(0), 5., 57., "render", "ui"),
            (Some(1), 27., 27., "layout", "ui"),
            (Some(1), 5., 25., "paint", "ui"),
            (Some(3), 20., 20., "draw_text", "fonts"),
            (Some(0), 2., 29., "process_events", "app"),
            (Some(5), 16., 16., "dispatch", "events"),
            (Some(5), 11., 11., "poll", "events"),
            (Some(0), 11., 11., "collect_metrics", "app"),
        ][index];
        Frame {
            id: index as u64,
            parent,
            self_value: own,
            total,
            label,
            package,
        }
    }
}

fn flame(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Application profile",
        &[
            Knob::new("Orientation", &["Flame", "Icicle"]),
            Knob::new("Colours", &["Name", "Package", "Heat"]),
            Knob::toggle("Highlight render"),
        ],
        |_, cx| {
            let view = cx.new(|cx| {
                FlameGraph::from_source(Arc::new(Profile), cx)
                    .unwrap_or_else(|error| panic!("Invalid demo profile {error:?}"))
            });
            let log = EventLog::new(cx);
            EventLog::watch(&log, &view, cx);
            (view, log, Cell::new([0, 0, 0]))
        },
        |(view, log, previous), knobs, _, cx| {
            let settings = [knobs.index(0), knobs.index(1), knobs.index(2)];
            if previous.replace(settings) != settings {
                let view = view.clone();
                cx.defer(move |cx| {
                    view.update(cx, |view, cx| {
                        view.set_orientation(
                            if settings[0] == 0 {
                                Orientation::Flame
                            } else {
                                Orientation::Icicle
                            },
                            cx,
                        );
                        view.set_colour_scheme(
                            match settings[1] {
                                1 => ColourScheme::Package,
                                2 => ColourScheme::Heat,
                                _ => ColourScheme::Name,
                            },
                            cx,
                        );
                        view.set_search(if settings[2] == 0 { "" } else { "render" }, cx);
                    })
                });
            }
            let height = view.read(cx).full_plot_height() + px(38.);
            div()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(div().h(height).child(view.clone()))
                .child(log.clone())
                .into_any_element()
        },
        window,
        cx,
    )
}

#[derive(Clone)]
struct Settings {
    children: Vec<NodeId>,
    checked: BTreeMap<NodeId, bool>,
    values: BTreeMap<NodeId, Value>,
    defaults: BTreeMap<NodeId, Value>,
    choices: Vec<String>,
}

impl Settings {
    fn new() -> Self {
        let values = BTreeMap::from([
            (1, Value::Text("Workstation".into())),
            (2, Value::Number(12.)),
            (3, Value::Bool(true)),
            (4, Value::Enum("Auto".into())),
            (5, Value::Colour(0x397fc8)),
            (7, Value::Text("1.0".into())),
            (9, Value::Vector([120., 80.])),
            (10, Value::Rect([0., 0., 640., 480.])),
            (11, Value::Flags(vec!["Auto".into()])),
            (12, Value::Path("workspace/settings.toml".into())),
        ]);
        Self {
            children: vec![1, 2, 3, 4, 5, 6, 9, 10, 11, 12],
            checked: BTreeMap::from([(3, true)]),
            defaults: values.clone(),
            values,
            choices: vec!["Auto".into(), "Manual".into()],
        }
    }
}

impl TreeSource for Settings {
    fn roots(&self) -> &[NodeId] {
        &[0, 8]
    }
    fn children(&self, id: NodeId) -> Children<'_> {
        match id {
            0 => Children::Ready(&self.children),
            6 => Children::Ready(&[7]),
            _ => Children::Ready(&[]),
        }
    }
    fn label(&self, id: NodeId) -> &str {
        [
            "Workspace",
            "Name",
            "History limit",
            "Autosave",
            "Mode",
            "Accent",
            "Advanced",
            "Format version",
            "Extensions",
            "Origin",
            "Bounds",
            "Capabilities",
            "Settings file",
        ]
        .get(id as usize)
        .copied()
        .unwrap_or("")
    }
}

impl TreeData for Settings {
    fn icon_element(&self, id: NodeId) -> Option<AnyElement> {
        let icon = if matches!(id, 0 | 6 | 8) {
            IconName::Folder
        } else {
            IconName::File
        };
        Some(Icon::new(icon).size(px(16.)).flex_none().into_any_element())
    }
    fn badge(&self, id: NodeId) -> Option<&str> {
        (id == 6).then_some("1 setting")
    }
    fn checked(&self, id: NodeId) -> Option<bool> {
        self.checked.get(&id).copied()
    }
    fn property(&self, id: NodeId) -> Option<Property<'_>> {
        Some(Property {
            value: self.values.get(&id)?,
            default: self.defaults.get(&id)?,
            choices: &self.choices,
            read_only: id == 7,
        })
    }
    fn property_options(&self, id: NodeId) -> PropertyOptions<'_> {
        match id {
            2 => PropertyOptions {
                description: Some("Maximum number of undo entries."),
                units: Some("entries"),
                min: Some(1.),
                max: Some(100.),
                step: Some(1.),
                ..Default::default()
            },
            1 => PropertyOptions {
                multiline_rows: Some(4),
                ..Default::default()
            },
            7 => PropertyOptions {
                advanced: true,
                ..Default::default()
            },
            9 => PropertyOptions {
                mixed: true,
                ..Default::default()
            },
            _ => PropertyOptions::default(),
        }
    }

    fn validate(&self, _: NodeId, value: &Value) -> Result<(), String> {
        if matches!(value, Value::Number(n) if !(1. ..=100.).contains(n)) {
            return Err("History limit must be between 1 and 100".to_owned());
        }
        Ok(())
    }
}

struct TreeDemo<const P: bool> {
    view: Entity<TreeView<P>>,
    log: Entity<EventLog>,
    settings: Settings,
    advanced: Cell<bool>,
    style: Cell<RowStyle>,
    tall_header: Cell<bool>,
    _subscription: gpui_kit::Subscription,
}

impl<const P: bool> TreeDemo<P> {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = Settings::new();
        let view = cx.new(|cx| {
            let mut view = TreeView::new(Rc::new(settings.clone()), cx)
                .focusable(true)
                .row_heights([(0, 44.)])
                .inspector_controls("showcase-property-grid", window, cx);
            view.set_expanded(0, true, cx);
            view
        });
        let log = EventLog::new(cx);
        EventLog::watch(&log, &view, cx);
        let subscription = cx.subscribe(&view, |this, view, event: &TreeEvent, cx| {
            match event {
                TreeEvent::Edited(id, value) => {
                    this.settings.values.insert(*id, value.clone());
                }
                TreeEvent::Checked(id, value) => {
                    this.settings.checked.insert(*id, *value);
                }
                TreeEvent::Reorder { node, before } => {
                    if !this.settings.children.contains(node)
                        || !this.settings.children.contains(before)
                    {
                        return;
                    }
                    this.settings.children.retain(|id| id != node);
                    if let Some(index) = this.settings.children.iter().position(|id| id == before) {
                        this.settings.children.insert(index, *node);
                    }
                }
                _ => return,
            }
            view.update(cx, |view, cx| {
                view.replace_source(Rc::new(this.settings.clone()), cx)
            });
        });
        Self {
            view,
            log,
            settings,
            advanced: Cell::new(false),
            style: Cell::new(RowStyle::default()),
            tall_header: Cell::new(true),
            _subscription: subscription,
        }
    }
}

fn tree<const P: bool>(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        if P {
            "Workspace properties"
        } else {
            "Workspace tree"
        },
        &[
            Knob::toggle("Expand advanced"),
            Knob::new("Row height", &["Compact", "Default", "Comfortable"]).default(1),
            Knob::toggle("Alternating rows"),
            Knob::toggle("Grid lines"),
            Knob::toggle("Indent guides"),
            Knob::toggle("Group styling"),
            Knob::toggle("Sticky groups"),
            Knob::toggle("Taller first group").default(1),
        ],
        |window, cx| cx.new(|cx| TreeDemo::<P>::new(window, cx)),
        |entity, knobs, _, cx| {
            let demo = entity.read(cx);
            let open = knobs.on(0);
            if demo.advanced.replace(open) != open {
                let view = demo.view.clone();
                cx.defer(move |cx| view.update(cx, |view, cx| view.set_expanded(6, open, cx)));
            }
            let style = RowStyle {
                density: match knobs.index(1) {
                    0 => RowDensity::Compact,
                    2 => RowDensity::Comfortable,
                    _ => RowDensity::Default,
                },
                alternating: knobs.on(2),
                grid_lines: knobs.on(3),
                indent_guides: knobs.on(4),
                group_headers: knobs.on(5),
                sticky_headers: knobs.on(6),
            };
            if demo.style.replace(style) != style {
                let view = demo.view.clone();
                cx.defer(move |cx| view.update(cx, |view, cx| view.set_row_style(style, cx)));
            }
            let tall = knobs.on(7);
            if demo.tall_header.replace(tall) != tall {
                let view = demo.view.clone();
                cx.defer(move |cx| {
                    view.update(cx, |view, cx| {
                        view.set_row_heights(tall.then_some((0, 44.)), cx);
                    });
                });
            }
            entity.clone().into_any_element()
        },
        window,
        cx,
    )
}

impl<const P: bool> gpui_kit::Render for TreeDemo<P> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.view.clone()).child(self.log.clone())
    }
}
