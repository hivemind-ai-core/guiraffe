//! Add new crate workloads here, then list them in `registered`.
//! The same functions feed the CLI, criterion and instruction-count benches.
use crate::Suite;
use guiraffe_core::{
    ColorName, Scheme, Variant,
    variant::{ResolveCtx, resolve},
};
use guiraffe_windows::placement::{Display, PlacementRules, Rect, cascade, place};
use std::hint::black_box;

/// Resolve every variant in both schemes, including color contrast work.
pub fn core_variants() {
    for scheme in [Scheme::Light, Scheme::Dark] {
        let ctx = black_box(ResolveCtx::new(scheme));
        for variant in [
            Variant::Filled,
            Variant::Light,
            Variant::Outline,
            Variant::Subtle,
            Variant::Transparent,
            Variant::White,
            Variant::Default,
        ] {
            black_box(resolve(
                &ctx,
                black_box(ColorName::Blue),
                black_box(variant),
            ));
        }
    }
}

/// Restore and cascade a window against two displays, including disconnected UUID recovery.
pub fn windows_placement() {
    let displays = [
        Display {
            uuid: None,
            bounds: Rect::new(0., 0., 1920., 1080.),
        },
        Display {
            uuid: None,
            bounds: Rect::new(1920., 0., 2560., 1440.),
        },
    ];
    let saved = black_box(Rect::new(1800., -50., 1000., 800.));
    let bounds = place(
        Some((saved, Some("disconnected"))),
        black_box(&displays),
        &PlacementRules::default(),
    );
    black_box(cascade(
        bounds,
        black_box(7),
        displays.get(1).map(|display| &display.bounds),
    ));
}

// Each flow invocation is described below. Fixture creation is paid only by
// warmup; the measured move/delete workloads restore their document each time.
fn flow_graph() -> &'static guiraffe_flow_core::Graph {
    use guiraffe_flow_core::{Edge, Graph, Node, Point};
    static GRAPH: std::sync::OnceLock<Graph> = std::sync::OnceLock::new();
    GRAPH.get_or_init(|| {
        let graph = Graph::from_parts(
            (0..5000).map(|id| {
                let mut n = Node::new(
                    id,
                    "node",
                    Point::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
                );
                if id == 1 {
                    n.parent = Some(0);
                }
                n
            }),
            (0..10000).map(|id| {
                Edge::new(
                    id,
                    id % 5000,
                    (id % 5000 + if id < 5000 { 1 } else { 100 }) % 5000,
                )
            }),
        );
        match graph {
            Ok(graph) => graph,
            Err(error) => panic!("flow benchmark fixture rejected: {error}"),
        }
    })
}
fn flow_editor(f: impl FnOnce(&mut guiraffe_flow_core::Editor)) {
    thread_local! {static EDITOR:std::cell::RefCell<Option<guiraffe_flow_core::Editor>>=const {std::cell::RefCell::new(None)};}
    EDITOR.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(
                match guiraffe_flow_core::Editor::new(flow_graph().clone()) {
                    Ok(mut e) => {
                        e.history_limit(1);
                        e
                    }
                    Err(error) => panic!("flow benchmark editor rejected: {error}"),
                },
            );
        }
        if let Some(editor) = slot.as_mut() {
            f(editor);
        } else {
            panic!("flow benchmark editor unavailable");
        }
    });
}
/// Build and drop one 5k-node/10k-edge scene, including validation/routes/BVHs.
pub fn flow_build() {
    let result = guiraffe_flow_core::Scene::new(black_box(flow_graph().clone()));
    assert!(result.is_ok());
    drop(black_box(result));
}
/// Twenty spatial queries plus cached-route access; reuse output buffers.
pub fn flow_query() {
    use guiraffe_flow_core::{Rect, Scene, Visible};
    static SCENE: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
    let scene = SCENE.get_or_init(|| match Scene::new(flow_graph().clone()) {
        Ok(s) => s,
        Err(error) => panic!("flow benchmark scene rejected: {error}"),
    });
    thread_local! {static VISIBLE:std::cell::RefCell<Visible>=std::cell::RefCell::new(Visible {nodes:Vec::with_capacity(5000),edges:Vec::with_capacity(10000),visited:0});}
    VISIBLE.with(|scratch| {
        let mut visible = scratch.borrow_mut();
        for i in 0..20 {
            scene.visible(
                black_box(Rect::new(i as f32 * 200., i as f32 * 100., 1920., 1080.)),
                &mut visible,
            );
            for id in &visible.edges {
                black_box(scene.paths().get(id));
            }
        }
    });
}
/// Move one child node, then undo; includes the edit/history/cache costs.
pub fn flow_move() {
    flow_editor(|editor| {
        let positions =
            std::collections::BTreeMap::from([(1, guiraffe_flow_core::Point::new(280., 20.))]);
        assert!(matches!(
            editor.move_nodes(black_box(&positions), None),
            Ok(true)
        ));
        assert!(matches!(editor.undo(), Ok(true)));
    });
}
/// Select/delete a child with incident edges, then undo; hierarchy depth cost included.
pub fn flow_delete_child() {
    flow_editor(|editor| {
        editor.select(&std::collections::BTreeSet::from([1]), false);
        assert!(matches!(editor.delete_selected(), Ok(true)));
        assert!(matches!(editor.undo(), Ok(true)));
    });
}

/// Warm the native fixture outside criterion/Callgrind measurement.
pub fn prepare_data_ui() {
    crate::data_ui::instantiate();
    crate::data_ui::patch();
    crate::data_ui::splice();
    crate::data_ui::structural_splice();
    crate::data_ui::structural_splice();
    crate::data_ui::settle();
}
pub fn data_ui_instantiate() {
    crate::data_ui::instantiate();
}
pub fn data_ui_patch() {
    crate::data_ui::patch();
}
pub fn data_ui_splice() {
    crate::data_ui::splice();
}

pub fn data_ui_structural_splice() {
    crate::data_ui::structural_splice();
}

/// Shared registration table consumed by the CLI, criterion and iai-callgrind.
/// Add workloads here once; each tool expands its own registration from this list.
#[macro_export]
macro_rules! with_suites {
    ($callback:ident) => {
        $callback! {
            (core_variants, "core-variants"),
            (flow_build,"flow-build-5k"),
            (flow_query,"flow-query-5k"),
            (flow_move,"flow-move-5k"),
            (flow_delete_child,"flow-delete-child-5k"),
            (windows_placement, "windows-placement"),
            (data_ui_instantiate, "data-ui-instantiate"),
            (data_ui_patch, "data-ui-patch"),
            (data_ui_splice, "data-ui-splice"),
            (data_ui_structural_splice, "data-ui-structural-splice"),
        }
    };
}
macro_rules! make_registry {
    ($(($function:ident, $name:literal),)*) => {
        /// Registered headless workloads. Names are stable baseline filenames.
        pub fn registered() -> Vec<Suite> {
            vec![$(Suite { name: $name, run: $function }),*]
        }
    };
}
crate::with_suites!(make_registry);
