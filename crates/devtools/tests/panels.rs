use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, Render, TestAppContext, Window, div,
    prelude::*, px, size,
};
use guiraffe_core::{Density, GuiraffeTheme, Scheme, StylePatch};
use guiraffe_devtools::*;
use guiraffe_test::{
    render::RenderCounter,
    snapshot::{SnapshotConfig, Snapshotter, assert_snapshot},
};
use std::{cell::Cell, rc::Rc};

struct AppView {
    tools: Devtools,
    renders: RenderCounter,
    focus: FocusHandle,
}
impl Focusable for AppView {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for AppView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        guiraffe_core::provide(
            StylePatch::new().density(Density::Compact),
            self.tools.track(
                "example-editor",
                div()
                    .id("editor")
                    .debug_selector(|| "editor".into())
                    .track_focus(&self.focus)
                    .size_full()
                    .child("Inspected app"),
            ),
        )
    }
}
struct Host {
    root: Entity<DevtoolsRoot>,
    count: Rc<Cell<usize>>,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.count.set(self.count.get() + 1);
        div().size_full().child(self.root.clone())
    }
}

#[gpui_kit::test]
fn keyboard_toggle_polling_and_controls_preserve_app_renders(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        init(cx);
        init(cx);
    });
    let renders = RenderCounter::new();
    let host_renders = Rc::new(Cell::new(0));
    let mut handle = None;
    let mut inspected_store = None;
    let mut inspected_app = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        window.resize(size(px(1000.), px(850.)));
        let tools = Devtools::new(cx);
        handle = Some(tools.clone());
        let store = guiraffe_state::Store::new(0u32, cx);
        #[derive(Debug)]
        struct Add;
        impl guiraffe_state::Event<u32> for Add {
            fn handle(self, fx: &mut guiraffe_state::Fx<'_, u32>) {
                fx.update(|n| *n += 1);
            }
        }
        let (adapter, _) =
            inspect_store("counter", &store, 16, |n| serde_json::json!({"n": n}), cx);
        tools.add_store(adapter, cx);
        store.dispatch(Add, cx);
        store.flush(cx);
        inspected_store = Some(store);
        let app = cx.new(|cx| AppView {
            tools: tools.clone(),
            renders: renders.clone(),
            focus: cx.focus_handle(),
        });
        app.read(cx).focus.clone().focus(window, cx);
        inspected_app = Some(app.clone());
        Host {
            root: root(app, &tools, cx),
            count: host_renders.clone(),
        }
    });
    let tools = handle.unwrap_or_else(|| panic!("tools"));
    vcx.run_until_parked();
    assert!(!tools.is_open());
    vcx.simulate_keystrokes("ctrl-shift-i");
    vcx.run_until_parked();
    assert!(tools.is_open(), "default keybinding opens overlay");
    let click = |vcx: &mut gpui_kit::VisualTestContext, id: &'static str| {
        let center = vcx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"))
            .center();
        vcx.simulate_click(center, gpui_kit::Modifiers::default());
        vcx.run_until_parked();
    };
    click(vcx, "devtools-highlight");
    click(vcx, "devtools-inspect");
    let editor = vcx
        .debug_bounds("editor")
        .unwrap_or_else(|| panic!("editor"));
    vcx.simulate_mouse_move(
        gpui_kit::point(px(5.), px(100.)),
        None,
        gpui_kit::Modifiers::default(),
    );
    assert_eq!(
        tools.inspect_at(editor.center()).map(|v| v.id.to_string()),
        Some("example-editor".into())
    );
    let before = renders.get();
    let host_before = host_renders.get();
    for _ in 0..5 {
        vcx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        vcx.run_until_parked();
    }
    if gpui_kit::GPUI_FAST {
        assert_eq!(renders.get(), before, "profiling does not render app");
        assert_eq!(
            host_renders.get(),
            host_before,
            "profiling does not render ancestor"
        );
    }
    let samples = vcx.update(|_, cx| tools.frame_samples(cx));
    assert!(!samples.is_empty(), "real GPUI draw events reach profiler");
    assert!(
        samples
            .iter()
            .all(|f| f.total >= f.layout + f.prepaint + f.paint)
    );
    // Pause/resume resets GPUI's trace cursor without touching the app entities.
    vcx.update(|_, cx| tools.set_paused(true, cx));
    vcx.run_until_parked();
    vcx.update(|_, cx| tools.set_paused(false, cx));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        vcx.run_until_parked();
    }
    assert!(
        vcx.update(|_, cx| tools.frame_samples(cx).len()) > samples.len(),
        "collection resumes after trace buffer reset"
    );
    if gpui_kit::GPUI_FAST {
        assert_eq!(renders.get(), before, "pause/resume does not render app");
    }
    // Native GPUI input updates the typed root even for a background click. Measure that
    // baseline separately; the zero-render assertion above covers tools-driven polling.
    let input_before = renders.get();
    vcx.simulate_click(
        gpui_kit::point(px(5.), px(100.)),
        gpui_kit::Modifiers::default(),
    );
    vcx.run_until_parked();
    let background_input_renders = renders.get() - input_before;
    let input_before = renders.get();
    click(vcx, "devtools-pause");
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            renders.get() - input_before,
            background_input_renders,
            "controls add no renders beyond native pointer input"
        );
    }
    let app = inspected_app.as_ref().unwrap_or_else(|| panic!("app"));
    let tracked_before = tools.views()[0].renders;
    vcx.update(|_, cx| cx.notify(app.entity_id()));
    vcx.run_until_parked();
    assert!(
        tools.views()[0].renders > tracked_before,
        "a real view render increments its counter"
    );
    click(vcx, "devtools-slow");
    assert_eq!(
        vcx.update(|_, cx| guiraffe_motion::MotionPrefs::global(cx).time_scale),
        10.
    );
    click(vcx, "devtools-slow");
    assert_eq!(
        vcx.update(|_, cx| guiraffe_motion::MotionPrefs::global(cx).time_scale),
        1.
    );
    click(vcx, "devtools-before-0-1");
    let store = inspected_store.as_ref().unwrap_or_else(|| panic!("store"));
    assert_eq!(
        vcx.update(|_, cx| *store.state(cx)),
        0,
        "panel Before jumps through real Store interceptor"
    );
    let infos = tools.views();
    assert_eq!(infos.len(), 1);
    assert!(infos[0].is_flashing_at(infos[0].last_render));
    assert!(!infos[0].is_flashing_at(infos[0].last_render + std::time::Duration::from_millis(180)));
    assert_eq!(infos[0].style.density, Density::Compact);
    assert!(infos[0].bounds.size.width > px(0.));
    vcx.simulate_keystrokes("ctrl-shift-i");
    vcx.run_until_parked();
    assert!(!tools.is_open());
    vcx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    vcx.run_until_parked();
    let callbacks = vcx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(callbacks, 0, "closed tools stop requesting frames");
}

#[gpui_kit::test]
fn store_interceptor_jump_and_eviction(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = guiraffe_state::Store::new(0u32, cx);
        #[derive(Debug)]
        struct Add;
        impl guiraffe_state::Event<u32> for Add {
            fn handle(self, fx: &mut guiraffe_state::Fx<'_, u32>) {
                fx.update(|n| *n += 1);
            }
        }
        let (inspect, recorder) =
            inspect_store("counter", &store, 2, |n| serde_json::json!({"n": n}), cx);
        store.dispatch(Add, cx);
        store.flush(cx);
        store.dispatch(Add, cx);
        store.flush(cx);
        assert_eq!(*store.state(cx), 2);
        assert_eq!(inspect.rows(0, 8)[1].changes[0].path, "/n");
        assert!(inspect.jump(1, false, cx));
        store.flush(cx);
        assert_eq!(*store.state(cx), 1);
        assert!(!inspect.jump(1, true, cx), "evicted events cannot jump");
        assert_eq!(recorder.0.borrow().entries().len(), 2);
        assert!(inspect.jump(2, true, cx));
        store.flush(cx);
        assert_eq!(*store.state(cx), 1);
    });
}

#[test]
fn panels_in_both_schemes() {
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut snap = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(move |cx| {
                guiraffe_core::init(cx);
                guiraffe_core::apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            });
        let image = snap
            .render(size(px(900.), px(850.)), |_, cx| {
                let tools = Devtools::new(cx);
                let store = guiraffe_state::Store::new(1u32, cx);
                let (inspect, _) = inspect_store(
                    "Example counter",
                    &store,
                    8,
                    |n| serde_json::json!({"count": n}),
                    cx,
                );
                #[derive(Debug)]
                struct Increment;
                impl guiraffe_state::Event<u32> for Increment {
                    fn handle(self, fx: &mut guiraffe_state::Fx<'_, u32>) {
                        fx.update(|n| *n += 1);
                    }
                }
                store.dispatch(Increment, cx);
                store.flush(cx);
                tools.add_store(inspect, cx);
                tools.set_open(true, cx);
                tools.set_paused(true, cx);
                tools.set_inspecting(true, cx);
                // Keep store alive through view capture.
                struct Example {
                    store: guiraffe_state::Store<u32>,
                    tools: Devtools,
                }
                impl Render for Example {
                    fn render(
                        &mut self,
                        _: &mut Window,
                        cx: &mut Context<Self>,
                    ) -> impl IntoElement {
                        let _ = &self.store;
                        self.tools.track(
                            "snapshot-app",
                            div()
                                .size_full()
                                .bg(guiraffe_core::GuiraffeTheme::global(cx)
                                    .surfaces()
                                    .body
                                    .to_hsla())
                                .text_color(
                                    guiraffe_core::GuiraffeTheme::global(cx)
                                        .surfaces()
                                        .text
                                        .to_hsla(),
                                )
                                .child("Example application"),
                        )
                    }
                }
                let app = cx.new(|_| Example {
                    store,
                    tools: tools.clone(),
                });
                root(app, &tools, cx)
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "panels-light"
        } else {
            "panels-dark"
        };
        assert_snapshot(
            name,
            &image,
            &SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
                .update_from_env(),
        )
        .unwrap_or_else(|e| panic!("{e:#}"));
    }
}

#[gpui_kit::test]
fn root_focus_supports_shortcut_without_a_focused_app_control(cx: &mut TestAppContext) {
    cx.update(guiraffe_core::init);
    let mut handle = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let tools = Devtools::new(cx);
        tools.focus(window, cx);
        handle = Some(tools.clone());
        let app = cx.new(|cx| AppView {
            tools: tools.clone(),
            renders: RenderCounter::new(),
            focus: cx.focus_handle(),
        });
        Host {
            root: root(app, &tools, cx),
            count: Rc::new(Cell::new(0)),
        }
    });
    let tools = handle.unwrap_or_else(|| panic!("tools"));
    vcx.simulate_keystrokes("ctrl-shift-i");
    vcx.run_until_parked();
    assert!(tools.is_open());
    vcx.simulate_keystrokes("ctrl-shift-i");
    vcx.run_until_parked();
    assert!(!tools.is_open());
}
