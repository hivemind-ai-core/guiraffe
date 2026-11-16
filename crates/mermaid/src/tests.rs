//! GPUI interaction tests for [`MermaidView`](crate::MermaidView).

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};

use crate::{host::Host, view::Status};

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Host>) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Point::default(),
                    size(px(900.0), px(700.0)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(Host::new),
        )
        .expect("window opens")
    })
}

#[track_caller]
fn expect_ready(state: &crate::MermaidViewState) {
    assert_eq!(
        state.status(),
        &Status::Ready,
        "not ready: {:?}",
        state.status()
    );
}

fn settle(cx: &mut TestAppContext) {
    cx.executor().run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
}

#[gpui_kit::test]
fn a_flowchart_renders_to_a_visible_image(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        expect_ready(state.read(cx));
        let image = state.read(cx).image().expect("image");
        assert!(image.image.size(0).width.0 > 1);
        assert!(image.image.size(0).height.0 > 1);
        assert!(state.read(cx).svg().expect("svg").contains("<text"));
    });
    // The element really lays out and paints.
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

#[gpui_kit::test]
fn an_invalid_diagram_shows_its_error_and_recovers_when_fixed(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        host.update(cx, |host, cx| host.set_source("notADiagram\n  ???", cx));
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        assert!(matches!(state.read(cx).status(), Status::Failed(_)));
        assert!(state.read(cx).image().is_none());
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("error frame renders");

    cx.update(|cx| {
        host.update(cx, |host, cx| {
            host.set_source("flowchart LR\n  A --> B", cx)
        });
    });
    settle(cx);
    cx.update(|cx| {
        expect_ready(host.read(cx).diagram.read(cx));
    });
}

#[gpui_kit::test]
fn rapid_source_changes_publish_only_the_final_revision(cx: &mut TestAppContext) {
    let (_, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        host.update(cx, |host, cx| {
            for label in ["First", "Second", "Final"] {
                host.set_source(format!("flowchart LR\n  Start --> {label}"), cx);
            }
        });
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        expect_ready(state.read(cx));
        let svg = state.read(cx).svg().expect("svg");
        assert!(svg.contains("Final"), "published the last source");
        assert!(!svg.contains("Second"));
    });
}

#[gpui_kit::test]
fn zoom_buttons_change_scale_and_fit_restores_the_whole_diagram(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    let (fitted, zoomed) = cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        let fitted = state.read(cx).viewport().zoom();
        state.update(cx, |state, cx| state.zoom_in(cx));
        (fitted, state.read(cx).viewport().zoom())
    });
    assert!(zoomed > fitted);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("zoomed frame renders");

    cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        state.update(cx, |state, cx| state.fit(cx));
        assert_eq!(state.read(cx).viewport().zoom(), fitted);
    });
}

#[gpui_kit::test]
fn wheel_scrolling_over_the_view_zooms(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    let before = cx.update(|cx| host.read(cx).diagram.read(cx).viewport().zoom());
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "diagram",
            gpui_kit::ScrollDelta::Lines(Point::new(0.0, 2.0)),
            cx,
        );
    })
    .expect("scroll dispatches");
    let after = cx.update(|cx| host.read(cx).diagram.read(cx).viewport().zoom());
    assert!(after > before, "{before} -> {after}");
}

#[gpui_kit::test]
fn double_click_fits_a_panned_diagram(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        state.update(cx, |state, cx| {
            state.zoom_in(cx);
            state.zoom_in(cx);
        });
    });
    let zoomed = cx.update(|cx| host.read(cx).diagram.read(cx).viewport().zoom());
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
    cx.update_window(handle, |_, window, cx| window.double_click("diagram", cx))
        .expect("double click dispatches");
    let after = cx.update(|cx| host.read(cx).diagram.read(cx).viewport().zoom());
    assert!(after < zoomed, "double click fits: {zoomed} -> {after}");
    assert!(cx.update(|cx| host.read(cx).diagram.read(cx).viewport().auto_fit()));
}

#[gpui_kit::test]
fn a_theme_change_redraws_the_diagram_in_the_new_palette(cx: &mut TestAppContext) {
    let (_, host) = open(cx);
    settle(cx);
    let light = cx.update(|cx| {
        host.read(cx)
            .diagram
            .read(cx)
            .svg()
            .expect("svg")
            .to_owned()
    });
    cx.update(|cx| guiraffe_core::set_scheme(guiraffe_core::Scheme::Dark, cx));
    cx.update(|cx| host.update(cx, Host::request_redraw));
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).diagram.clone();
        expect_ready(state.read(cx));
        let dark = state.read(cx).svg().expect("svg");
        assert_ne!(light, dark, "the theme changed the drawing");
    });
}
