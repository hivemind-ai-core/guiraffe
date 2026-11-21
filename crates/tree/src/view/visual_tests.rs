//! Goldens for the row-style presets and the open editors, in both themes. Each scene drives
//! private state directly (scroll offset, an open picker, a finished search) so the image shows
//! exactly one option per golden.
use super::*;
use gpui_kit::size;
use guiraffe_core::Scheme;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

/// Two groups of typed properties; the first has a nested group so guides have two levels.
struct Inspector {
    values: Vec<(Value, Value)>,
    choices: [String; 2],
}

const LABELS: [&str; 18] = [
    "Appearance",
    "Name",
    "Opacity",
    "Accent",
    "Mode",
    "Visible",
    "Layout",
    "Origin",
    "Bounds",
    "Editor",
    "Font size",
    "Tab width",
    "Line numbers",
    "Theme file",
    "Word wrap",
    "Ruler",
    "Minimap",
    "Cursor width",
];

impl Inspector {
    fn new() -> Self {
        let values = vec![
            (
                Value::Text("Inspector".into()),
                Value::Text("Inspector".into()),
            ),
            (Value::Number(80.), Value::Number(100.)),
            (Value::Colour(0x397fc8), Value::Colour(0x397fc8)),
            (Value::Enum("Auto".into()), Value::Enum("Auto".into())),
            (Value::Bool(true), Value::Bool(true)),
            (Value::Vector([120., 80.]), Value::Vector([120., 80.])),
            (
                Value::Rect([0., 0., 640., 480.]),
                Value::Rect([0., 0., 640., 480.]),
            ),
            (Value::Number(13.), Value::Number(13.)),
            (Value::Number(4.), Value::Number(4.)),
            (Value::Bool(true), Value::Bool(true)),
            (
                Value::Path("themes/dark.toml".into()),
                Value::Path("themes/dark.toml".into()),
            ),
            (Value::Bool(false), Value::Bool(false)),
            (Value::Number(100.), Value::Number(100.)),
            (Value::Bool(true), Value::Bool(true)),
            (Value::Number(2.), Value::Number(2.)),
        ];
        Self {
            values,
            choices: ["Auto".into(), "Manual".into()],
        }
    }

    /// Index into `values` for a property node (groups have none).
    fn slot(id: NodeId) -> Option<usize> {
        match id {
            1..=5 => Some(id as usize - 1),
            7 | 8 => Some(id as usize - 2),
            10..=17 => Some(id as usize - 3),
            _ => None,
        }
    }
}

impl TreeSource for Inspector {
    fn roots(&self) -> &[NodeId] {
        &[0, 9]
    }
    fn children(&self, id: NodeId) -> Children<'_> {
        Children::Ready(match id {
            0 => &[1, 2, 3, 4, 5, 6],
            6 => &[7, 8],
            9 => &[10, 11, 12, 13, 14, 15, 16, 17],
            _ => &[],
        })
    }
    fn label(&self, id: NodeId) -> &str {
        LABELS.get(id as usize).copied().unwrap_or("")
    }
}

impl TreeData for Inspector {
    fn badge(&self, id: NodeId) -> Option<&str> {
        (id == 6).then_some("2")
    }
    fn checked(&self, id: NodeId) -> Option<bool> {
        matches!(id, 5 | 12).then_some(true)
    }
    fn property(&self, id: NodeId) -> Option<Property<'_>> {
        let (value, default) = self.values.get(Self::slot(id)?)?;
        Some(Property {
            value,
            default,
            choices: &self.choices,
            read_only: false,
        })
    }
    fn property_options(&self, id: NodeId) -> PropertyOptions<'_> {
        match id {
            2 => PropertyOptions {
                units: Some("%"),
                min: Some(0.),
                max: Some(100.),
                step: Some(1.),
                ..Default::default()
            },
            10 => PropertyOptions {
                units: Some("px"),
                min: Some(6.),
                max: Some(72.),
                step: Some(1.),
                ..Default::default()
            },
            _ => PropertyOptions::default(),
        }
    }
}

fn config() -> SnapshotConfig {
    SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots")).update_from_env()
}

fn theme_name(scheme: Scheme) -> &'static str {
    if scheme == Scheme::Light {
        "light"
    } else {
        "dark"
    }
}

/// Render a PropertyGrid (or TreeList) built by `build` in a fresh headless window and compare
/// it with `name-<theme>` for both themes.
fn golden<const P: bool>(
    name: &str,
    window_size: (f32, f32),
    build: impl Fn(TreeView<P>, &mut Window, &mut Context<TreeView<P>>) -> TreeView<P> + Copy + 'static,
) {
    let mut failures = Vec::new();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let image = Snapshotter::new()
            .unwrap_or_else(|error| panic!("{error}"))
            .with_setup(move |cx| {
                gpui_kit::base::init(cx);
                gpui_kit::component::init(cx);
                guiraffe_core::apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            })
            .render(
                size(px(window_size.0), px(window_size.1)),
                move |window, cx| {
                    let view = cx.new(|cx| {
                        let mut view = TreeView::<P>::new(Rc::new(Inspector::new()), cx);
                        view.set_expanded(0, true, cx);
                        view.set_expanded(6, true, cx);
                        view.set_expanded(9, true, cx);
                        build(view, window, cx)
                    });
                    cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
                },
            )
            .unwrap_or_else(|error| panic!("{name}: {error:#}"));
        let tag = format!("{name}-{}", theme_name(scheme));
        if let Err(error) = assert_snapshot(&tag, &image, &config()) {
            failures.push(format!("{tag}: {error:#}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const GRID: (f32, f32) = (640., 300.);

#[test]
fn compact_alternating_rows() {
    golden::<true>(
        "property-grid/compact-alternating",
        GRID,
        |mut view, _, cx| {
            view.set_row_style(
                RowStyle {
                    density: RowDensity::Compact,
                    alternating: true,
                    ..Default::default()
                },
                cx,
            );
            view
        },
    );
}

#[test]
fn comfortable_grid_lines_guides_and_groups() {
    golden::<true>(
        "property-grid/comfortable-lines",
        GRID,
        |mut view, _, cx| {
            view.set_row_style(
                RowStyle {
                    density: RowDensity::Comfortable,
                    grid_lines: true,
                    indent_guides: true,
                    group_headers: true,
                    ..Default::default()
                },
                cx,
            );
            view
        },
    );
}

#[test]
fn search_highlights_matches() {
    golden::<true>(
        "property-grid/search-match",
        (640., 340.),
        |view, window, cx| {
            let mut view = view.inspector_controls("visual-search", window, cx);
            view.search_now("o", window, cx);
            view
        },
    );
}

#[test]
fn sticky_header_is_pushed_off_by_the_next_group() {
    golden::<true>("property-grid/sticky-push-off", GRID, |mut view, _, cx| {
        view.set_row_style(
            RowStyle {
                group_headers: true,
                sticky_headers: true,
                ..Default::default()
            },
            cx,
        );
        // "Editor" starts at row 9; leave half of the sticky "Appearance" header visible.
        view.scroll = view.row_top(9) - ROW / 2.;
        view.clamp_scroll();
        view
    });
}

#[test]
fn colour_picker_open() {
    golden::<true>(
        "property-grid/colour-picker",
        (640., 560.),
        |view, window, cx| {
            let mut view = view.split_ratio(0.4);
            view.open_colour_picker(3, window, cx);
            view
        },
    );
}

#[test]
fn range_slider_open() {
    golden::<true>("property-grid/slider", GRID, |view, _, cx| {
        let mut view = view.split_ratio(0.35);
        view.open_slider(2, cx);
        view
    });
}

#[test]
fn tree_list_shared_row_styles() {
    golden::<false>("tree-list/styles", (420., 300.), |mut view, _, cx| {
        view.set_row_style(
            RowStyle {
                density: RowDensity::Compact,
                alternating: true,
                grid_lines: true,
                indent_guides: true,
                ..Default::default()
            },
            cx,
        );
        view
    });
}
