//! Mid-drag golden for the Kanban demo: the drag preview is the theme's translucent
//! drag-preview token (spec/00 §12.1), so the card beneath stays visible through it.
//!
//! The snapshot helper only renders, so this drives the headless window by hand: press on a
//! card, move past the drag threshold, draw, capture.
use std::sync::Arc;

use gpui_kit::{
    HeadlessAppContext, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, PlatformInput,
    Point, point, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, assert_snapshot};

/// Logical coordinates in the 1100x760 showcase window: the "Draft release notes" card
/// (Backlog, second card) and a point over the "Verify drag and drop" card (In progress), so
/// the ghost overlaps a card and its translucency shows.
const GRAB: (f32, f32) = (390., 327.);
const OVER: (f32, f32) = (560., 268.);

fn at((x, y): (f32, f32)) -> Point<gpui_kit::Pixels> {
    point(px(x), px(y))
}

#[test]
fn kanban_mid_drag_preview_is_translucent() -> anyhow::Result<()> {
    if std::env::var("SHOWCASE_ONLY")
        .ok()
        .is_some_and(|only| !"kanban".contains(&only))
    {
        return Ok(());
    }
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    let mut failures = Vec::new();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            Arc::new(guiraffe_icons::GuiraffeAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        let window = cx.open_window(size(px(1100.), px(760.)), move |window, cx| {
            guiraffe_showcase::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
            guiraffe_showcase::snapshot_window_root(Some("Kanban"), window, cx)
        })?;
        let frame = |cx: &mut HeadlessAppContext| -> anyhow::Result<()> {
            cx.update_window(window.into(), |_, window, cx| {
                let _ = window.draw(cx);
            })?;
            cx.run_until_parked();
            Ok(())
        };
        frame(&mut cx)?;
        frame(&mut cx)?;
        let events = [
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(GRAB),
                pressed_button: None,
                modifiers: Modifiers::default(),
            }),
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(GRAB),
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(OVER),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }),
        ];
        for event in events {
            cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(event, cx);
            })?;
            frame(&mut cx)?;
        }
        // Let the sortable springs settle before capturing.
        for _ in 0..30 {
            cx.advance_clock(std::time::Duration::from_millis(16));
            frame(&mut cx)?;
        }
        let image = cx.capture_screenshot(window.into())?;
        let tag = format!(
            "kanban/mid-drag-{}",
            if scheme == Scheme::Light {
                "light"
            } else {
                "dark"
            }
        );
        if let Err(err) = assert_snapshot(&tag, &image, &cfg) {
            failures.push(format!("{tag}: {err:#}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
