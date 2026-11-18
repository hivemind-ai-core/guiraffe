//! Renders every showcase entry headlessly in light and dark (catches panics and blank demos)
//! and keeps a golden per entry so visual regressions in any component show up here.
use gpui_kit::{px, size};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_showcase::catalog::entries;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[test]
fn every_entry_renders_in_both_themes() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    let only = std::env::var("SHOWCASE_ONLY").ok();
    let mut failures = Vec::new();
    for e in entries() {
        if only.as_deref().is_some_and(|o| !slug(e.name).contains(o)) {
            continue;
        }
        for scheme in [Scheme::Light, Scheme::Dark] {
            let name = e.name;
            let mut s = Snapshotter::new().unwrap_or_else(|err| panic!("{err}"));
            let img = s
                .render(size(px(1100.), px(760.)), move |window, cx| {
                    guiraffe_showcase::init(cx);
                    apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                    guiraffe_showcase::window_root(Some(name), window, cx)
                })
                .unwrap_or_else(|err| panic!("{name}: {err:#}"));
            let tag = format!(
                "{}/{}",
                slug(e.name),
                if scheme == Scheme::Light {
                    "light"
                } else {
                    "dark"
                }
            );
            if let Err(err) = assert_snapshot(&tag, &img, &cfg) {
                failures.push(format!("{tag}: {err:#}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn recipes_match_the_generated_screen_schema() {
    let schema = serde_json::from_str(guiraffe_ui::catalogue_artifacts::SCREEN_SCHEMA)
        .unwrap_or_else(|error| panic!("{error}"));
    let validator = jsonschema::validator_for(&schema).unwrap_or_else(|error| panic!("{error}"));
    for (name, raw) in guiraffe_showcase::recipes::DOCUMENTS {
        let document = serde_json::from_str(raw).unwrap_or_else(|error| panic!("{name}: {error}"));
        let issues: Vec<_> = validator
            .iter_errors(&document)
            .map(|error| error.to_string())
            .collect();
        assert!(issues.is_empty(), "{name}: {issues:?}");
    }
}

#[test]
fn descriptor_examples_paint_the_same_with_reordered_props() {
    use gpui_kit::{AnyElement, App, Window};
    use guiraffe_ui::{
        DataUi, Document, NoLogic, Registry,
        widget::{NodeView, Schema, Widget},
    };
    use std::sync::Arc;

    struct Reordered {
        schema: &'static Schema,
        original: Arc<dyn Widget>,
    }

    impl Widget for Reordered {
        fn schema(&self) -> &'static Schema {
            self.schema
        }

        fn render(
            &self,
            node: &NodeView<'_>,
            children: Vec<AnyElement>,
            window: &mut Window,
            cx: &mut App,
        ) -> AnyElement {
            self.original.render(node, children, window, cx)
        }
    }

    for spec in guiraffe_ui::catalog::WIDGETS {
        let mut document: Document = serde_json::from_value(spec.example())
            .unwrap_or_else(|error| panic!("{}: {error}", spec.id));
        // Exercise visible prop values so swapping a prop with an unused slot changes the painting.
        for prop in spec.props {
            let value = match prop.name {
                "text" | "label" | "placeholder" => serde_json::json!("Example"),
                "diameter" => serde_json::json!(96),
                "order" => serde_json::json!(1),
                "value" if spec.id == "select" => serde_json::json!("Selected"),
                "options" if spec.id == "select" => serde_json::json!(["Selected"]),
                "value" if prop.type_name == "boolean" => serde_json::json!(true),
                _ => continue,
            };
            document.root.props.insert(prop.name.to_owned(), value);
        }
        let mut images = Vec::new();
        for reorder in [false, true] {
            let mut registry = Registry::with_builtins();
            if reorder {
                let original = registry
                    .get(spec.id)
                    .unwrap_or_else(|| panic!("{}", spec.id));
                let mut props = original.schema().props.to_vec();
                props.reverse();
                props.insert(0, "unused-test-prop");
                let schema = Box::leak(Box::new(Schema {
                    props: props.leak(),
                    events: original.schema().events,
                }));
                registry.register(spec.id, Reordered { schema, original });
            }
            let document = document.clone();
            let registry = Arc::new(registry);
            let image = Snapshotter::new()
                .unwrap_or_else(|error| panic!("{error}"))
                .render(size(px(320.), px(160.)), move |_, cx| {
                    guiraffe_showcase::init(cx);
                    DataUi::new(&document, registry, NoLogic, cx)
                        .unwrap_or_else(|error| panic!("{error}"))
                })
                .unwrap_or_else(|error| panic!("{}: {error}", spec.id));
            images.push(image);
        }
        assert!(
            images[0] == images[1],
            "{} changed its rendered output when props moved",
            spec.id
        );
    }
}
