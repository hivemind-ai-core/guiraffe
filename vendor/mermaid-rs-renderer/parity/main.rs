use std::{fs, path::Path, time::Instant};

macro_rules! evaluate {
    ($renderer:ident, $source:expr, $theme:ident, $font:expr) => {{
        let start = Instant::now();
        let mut theme = $renderer::Theme::$theme();
        if let Some(size) = $font {
            theme.font_size = size;
        }
        let config = $renderer::LayoutConfig::default();
        $renderer::parse_mermaid_strict($source).map(|parsed| {
            let layout = $renderer::compute_layout(&parsed.graph, &theme, &config);
            let elapsed = start.elapsed();
            let edge_coordinates: Vec<_> = layout
                .edges
                .iter()
                .flat_map(|edge| edge.points.iter().map(|&(x, y)| (x.to_bits(), y.to_bits())))
                .collect();
            let svg = $renderer::render_svg(&layout, &theme, &config);
            ((format!("{layout:?}"), edge_coordinates, svg), elapsed)
        })
    }};
}

fn check(name: &str, source: &str, dark: bool) -> bool {
    let font = (!name.ends_with(".mmd")).then_some(14.0);
    let (reference, patched) = if dark {
        (
            evaluate!(reference, source, dark, font),
            evaluate!(patched, source, dark, font),
        )
    } else {
        (
            evaluate!(reference, source, modern, font),
            evaluate!(patched, source, modern, font),
        )
    };
    match (reference, patched) {
        (Ok((expected, before)), Ok((actual, after))) => {
            assert!(expected.0 == actual.0, "layout differs for {name}");
            assert!(expected.1 == actual.1, "coordinate bits differ for {name}");
            assert!(expected.2 == actual.2, "SVG differs for {name}");
            println!("{name} dark={dark}: {before:?} → {after:?}");
            true
        }
        (Err(expected), Err(actual)) => {
            assert_eq!(expected.to_string(), actual.to_string(), "{name}");
            println!("{name}: parser rejects in both builds");
            false
        }
        _ => panic!("parse outcome differs for {name}"),
    }
}

fn check_fixtures(dir: &Path) -> usize {
    let mut count = 0;
    for entry in fs::read_dir(dir).expect("fixture directory") {
        let path = entry.expect("fixture").path();
        if path.is_dir() {
            count += check_fixtures(&path);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("mmd") {
            count += usize::from(check(
                path.to_str().expect("UTF-8 fixture path"),
                &fs::read_to_string(&path).expect("fixture source"),
                false,
            ));
        }
    }
    count
}

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.iter().any(|arg| arg == "--profile-patched") {
        let source = soak_source();
        let (_, elapsed) = evaluate!(patched, &source, modern, Some(14.0)).expect("soak diagram");
        println!("patched soak parse + layout: {elapsed:?}");
        return;
    }
    if arguments.iter().any(|arg| arg == "--profile-reference") {
        let source = soak_source();
        let (_, elapsed) = evaluate!(reference, &source, modern, Some(14.0)).expect("soak diagram");
        println!("reference soak parse + layout: {elapsed:?}");
        return;
    }
    if arguments.iter().any(|arg| arg == "--profile-staged") {
        check_staged(&soak_source(), None, false);
        return;
    }
    let write_goldens = arguments.iter().any(|arg| arg == "--write-staged-goldens");
    check_staged(
        include_str!(concat!(
            env!("GUIRAFFE_ROOT"),
            "/crates/mermaid/tests/fixtures/staged-flowchart.mmd"
        )),
        Some(Path::new("crates/mermaid/tests/fixtures")),
        write_goldens,
    );
    let count = check_fixtures(Path::new("vendor/mermaid-rs-renderer/tests/fixtures"));
    assert!(count > 0);
    println!("{count} valid upstream fixtures matched");
    let showcase = fs::read_to_string("crates/showcase/src/demos/mermaid.rs").expect("showcase");
    for name in ["FLOW", "SEQUENCE"] {
        let declaration = format!("const {name}: &str = \"");
        let source = showcase
            .split_once(&declaration)
            .and_then(|(_, rest)| rest.split_once("\";"))
            .expect("showcase diagram")
            .0;
        for dark in [false, true] {
            assert!(check(name, source, dark));
        }
    }
    if std::env::args().any(|arg| arg == "--soak") {
        assert!(check("soak-500", &soak_source(), false));
    }
}

fn soak_source() -> String {
    use std::fmt::Write;
    let mut source = String::from("flowchart TD\n");
    for i in 0..500 {
        writeln!(source, "  n{i}[Node {i}]").expect("string write");
    }
    for i in 1..500 {
        writeln!(source, "  n{} --> n{i}", (i - 1) / 3).expect("string write");
        if i % 10 == 0 {
            writeln!(source, "  n{i} -.-> n{}", (i * 7) % 500).expect("string write");
        }
    }
    source
}

fn check_staged(source: &str, goldens: Option<&Path>, write_goldens: bool) {
    use std::cell::Cell;
    let start = Instant::now();
    let mut theme = patched::Theme::modern();
    theme.font_size = 14.0;
    let config = patched::LayoutConfig {
        fast_text_metrics: goldens.is_some(),
        ..Default::default()
    };
    let graph = patched::parse_mermaid_strict(source)
        .expect("staged fixture")
        .graph;
    let mut preview = None;
    let mut preview_svg = None;
    let refined = patched::compute_flowchart_staged(
        &graph,
        &theme,
        &config,
        &mut |layout| {
            let svg = patched::render_svg(layout, &theme, &config);
            println!("staged parse + layout + preview SVG: {:?}", start.elapsed());
            preview = Some(layout.clone());
            preview_svg = Some(svg);
        },
        &|| false,
    )
    .expect("refined frame");
    println!("staged parse + refined layout: {:?}", start.elapsed());
    let preview = preview.expect("preview frame");
    let id = preview
        .nodes
        .iter()
        .find(|(_, node)| !node.hidden)
        .expect("anchor")
        .0;
    let before = &preview.nodes[id];
    let after = &refined.nodes[id];
    assert_eq!(preview.nodes.len(), refined.nodes.len());
    for (id, node) in &preview.nodes {
        let refined = &refined.nodes[id];
        assert_eq!((node.width, node.height), (refined.width, refined.height));
        assert!(
            ((node.x - before.x) - (refined.x - after.x)).abs() <= 0.02,
            "x moved for {id}"
        );
        assert!(
            ((node.y - before.y) - (refined.y - after.y)).abs() <= 0.02,
            "y moved for {id}"
        );
    }

    let final_svg = patched::render_svg(&refined, &theme, &config);
    let direct = patched::compute_layout(&graph, &theme, &config);
    assert!(final_svg == patched::render_svg(&direct, &theme, &config));
    if let Some(dir) = goldens {
        for (mode, svg) in [
            ("preview", preview_svg.expect("preview SVG")),
            ("refined", final_svg),
        ] {
            let path = dir.join(format!("staged-{mode}.svg"));
            if write_goldens {
                fs::write(&path, svg).expect("write golden");
            } else {
                assert!(
                    fs::read_to_string(&path).expect("read golden") == svg,
                    "{mode} SVG differs"
                );
            }
        }
    }
    let cancelled = Cell::new(false);
    let outcome = patched::compute_flowchart_staged(
        &graph,
        &theme,
        &config,
        &mut |_| cancelled.set(true),
        &|| cancelled.get(),
    );
    assert!(cancelled.get());
    assert!(outcome.is_none());
}
