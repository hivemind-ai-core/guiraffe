//! Real native gestures and shell invalidation, on either GPUI backend.
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext, Context, Entity, Focusable, IntoElement, Modifiers, MouseButton,
    ParentElement, Render, Role, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use guiraffe_timeline::*;
use std::{cell::Cell, rc::Rc};
struct Shell<V: 'static> {
    view: Entity<V>,
    renders: Rc<Cell<usize>>,
}
impl<V: Render> Render for Shell<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().child(
            self.view
                .clone()
                .cached(gpui_kit::StyleRefinement::default().size_full()),
        )
    }
}
struct WindowRoot<V: 'static> {
    shell: Entity<Shell<V>>,
}
impl<V: Render> Render for WindowRoot<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            self.shell
                .clone()
                .cached(gpui_kit::StyleRefinement::default().size_full()),
        )
    }
}
fn mount<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut App) -> Entity<V>,
) -> (Entity<V>, Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::component::init(cx);
        guiraffe_core::apply(Default::default(), cx);
    });
    let mut editor = None;
    let renders = Rc::new(Cell::new(0));
    let count = renders.clone();
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let view = build(cx);
        editor = Some(view.clone());
        let shell = cx.new(|_| Shell {
            view,
            renders: count,
        });
        WindowRoot { shell }
    });
    vcx.run_until_parked();
    (editor.expect("editor"), renders, vcx)
}
fn click(vcx: &mut VisualTestContext, p: gpui_kit::Point<gpui_kit::Pixels>, shift: bool) {
    vcx.simulate_click(
        p,
        Modifiers {
            shift,
            ..Default::default()
        },
    );
    vcx.run_until_parked();
}
fn drag(
    vcx: &mut VisualTestContext,
    a: gpui_kit::Point<gpui_kit::Pixels>,
    b: gpui_kit::Point<gpui_kit::Pixels>,
) {
    vcx.simulate_mouse_down(a, MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(b, Some(MouseButton::Left), Default::default());
    vcx.simulate_mouse_up(b, MouseButton::Left, Default::default());
    vcx.run_until_parked();
}
fn dope(cx: &mut App) -> Entity<DopeSheet> {
    DopeSheet::new(
        vec![Channel::group(
            "Object",
            vec![
                Channel::new("x", vec![0.5, 1.]),
                Channel::new("y", vec![0.7, 1.3]),
            ],
        )],
        TimeFormat::Frames(10.),
        3.,
        cx,
    )
}
fn dope_at(
    v: &Entity<DopeSheet>,
    vcx: &mut VisualTestContext,
    row: usize,
    time: f64,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    let b = vcx.debug_bounds("dope-body").expect("body");
    let x = vcx.update(|_, cx| v.read(cx).axis().x(time));
    point(
        b.left() + px(x as f32),
        b.top() + px(row as f32 * guiraffe_timeline::dope_sheet::ROW_H + 14.),
    )
}
#[gpui_kit::test]
fn dope_pointer_multiselect_moves_together_and_keeps_shell(cx: &mut TestAppContext) {
    let (view, _renders, vcx) = mount(cx, dope);
    let a = dope_at(&view, vcx, 1, 0.5);
    click(vcx, a, false);
    let b = dope_at(&view, vcx, 2, 0.7);
    click(vcx, b, true);
    assert_eq!(vcx.update(|_, cx| view.read(cx).model().selected.len()), 2);
    let target = dope_at(&view, vcx, 1, 1.5);
    drag(vcx, a, target);
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels()[1].keys().to_vec()),
        vec![1., 1.5]
    );
    let y = vcx.update(|_, cx| view.read(cx).channels()[2].keys().to_vec());
    assert!((y[1] - 1.7).abs() < 1e-10);
}
#[gpui_kit::test]
fn dope_box_selection_channel_controls_and_global_lock(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, dope);
    let a = dope_at(&view, vcx, 1, 0.2);
    let b = dope_at(&view, vcx, 2, 1.2);
    drag(vcx, a, b);
    assert_eq!(vcx.update(|_, cx| view.read(cx).model().selected.len()), 3);
    for (selector, kind) in [
        ("channel-mute-1", 0),
        ("channel-lock-1", 1),
        ("channel-visible-1", 2),
    ] {
        let p = vcx.debug_bounds(selector).expect(selector).center();
        click(vcx, p, false);
        assert!(vcx.update(|_, cx| {
            let c = &view.read(cx).channels()[1];
            match kind {
                0 => c.muted,
                1 => c.locked,
                _ => !c.visible,
            }
        }));
    }
    let collapse = vcx
        .debug_bounds("channel-expand-0")
        .expect("expand")
        .center();
    click(vcx, collapse, false);
    assert_eq!(vcx.update(|_, cx| view.read(cx).model().rows().len()), 1);
    click(vcx, collapse, false);
    vcx.update(|_, cx| view.update(cx, |v, cx| v.set_editable(false, cx)));
    vcx.run_until_parked();
    let before = vcx.update(|_, cx| view.read(cx).channels().to_vec());
    let a = dope_at(&view, vcx, 2, 0.7);
    let b = dope_at(&view, vcx, 2, 1.8);
    drag(vcx, a, b);
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels().to_vec()),
        before
    );
}
#[gpui_kit::test]
fn dope_keyboard_is_opt_in_and_per_channel_locks_survive(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, dope);
    let a = dope_at(&view, vcx, 1, 0.5);
    click(vcx, a, false);
    vcx.simulate_keystrokes("delete");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels()[1].keys().len()),
        2
    );
    vcx.update(|_, cx| view.update(cx, |v, cx| v.set_focusable(true, cx)));
    vcx.run_until_parked();
    click(vcx, a, false);
    assert!(
        !vcx.update(|w, cx| view.read(cx).focus_handle(cx).is_focused(w)),
        "pointer must not take focus even when opted in"
    );
    vcx.update(|w, cx| {
        let focus = view.read(cx).focus_handle(cx);
        w.focus(&focus, cx);
    });
    vcx.simulate_keystrokes("right");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels()[1].keys()[0]),
        0.6
    );
    vcx.simulate_keystrokes("ctrl-d");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels()[1].keys().len()),
        3
    );
    vcx.simulate_keystrokes("delete");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels()[1].keys().len()),
        2
    );
    vcx.update(|_, cx| view.update(cx, |v, cx| v.set_locked(1, true, cx)));
    vcx.simulate_keystrokes("ctrl-a delete");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).channels()[1].keys().len()),
        2
    );
    assert!(vcx.update(|_, cx| view.read(cx).channels()[2].keys().is_empty()));
}
#[gpui_kit::test]
fn bar_sparse_pointer_path_draws_every_bar_and_lock_blocks(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, |cx| BarCurveView::new(BarCurve::new(32, 0., 1.), cx));
    let b = vcx.debug_bounds("bar-curve").expect("bars");
    let a = point(b.left() + b.size.width / 64., b.bottom() - px(1.));
    let end = point(b.right() - b.size.width / 64., b.top() + px(1.));
    drag(vcx, a, end);
    let values = vcx.update(|_, cx| view.read(cx).model().values().to_vec());
    assert_eq!(values.len(), 32);
    assert!(
        values.windows(2).all(|v| v[1] > v[0]),
        "no gaps between sparse events"
    );
    vcx.update(|_, cx| view.update(cx, |v, cx| v.set_editable(false, cx)));
    vcx.run_until_parked();
    drag(vcx, end, a);
    assert_eq!(
        vcx.update(|_, cx| view.read(cx).model().values().to_vec()),
        values
    );
}
fn curves(cx: &mut App) -> Entity<CurveView> {
    let mut c = Curve::default();
    let mut k = Key::new(0.5, 0.5, Interp::Bezier);
    k.in_handle = (-0.2, -0.2);
    k.out_handle = (0.2, 0.2);
    c.set(k);
    c.set(Key::new(1.5, 0.8, Interp::Linear));
    CurveView::new(c, 0., 2., 0., 1., cx)
}
fn curve_at(
    v: &Entity<CurveView>,
    vcx: &mut VisualTestContext,
    t: f64,
    value: f64,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    let b = vcx.debug_bounds("curve-body").expect("curve");
    let (x, y) = vcx.update(|_, cx| {
        let v = v.read(cx);
        (v.axis().x(t), v.range().y(value))
    });
    point(b.left() + px(x as f32), b.top() + px(y as f32))
}
#[gpui_kit::test]
fn curve_key_and_tangent_pointer_edits_keep_shell(cx: &mut TestAppContext) {
    let (view, _renders, vcx) = mount(cx, curves);
    let a = curve_at(&view, vcx, 0.5, 0.5);
    click(vcx, a, false);
    assert_eq!(vcx.update(|_, cx| view.read(cx).selection()), Some((0, 0)));
    let h = curve_at(&view, vcx, 0.7, 0.7);
    let to = curve_at(&view, vcx, 0.9, 0.6);
    drag(vcx, h, to);
    let key = vcx.update(|_, cx| view.read(cx).curve().keys()[0]);
    assert!((key.out_handle.0 - 0.4).abs() < 0.02);
    let a = curve_at(&view, vcx, 0.5, 0.5);
    let b = curve_at(&view, vcx, 0.6, 0.4);
    drag(vcx, a, b);
    let key = vcx.update(|_, cx| view.read(cx).curve().keys()[0]);
    assert!((key.t - 0.6).abs() < 0.02);
    assert!((key.v - 0.4).abs() < 0.02);
}
#[gpui_kit::test]
fn external_curve_api_multi_visibility_and_segment_edits(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, curves);
    vcx.update(|_, cx| {
        view.update(cx, |v, cx| {
            assert_eq!(v.insert_at(1., cx), Some(1));
            assert!(v.select_key(0, 1, cx));
            assert!(v.set_interpolation(Interp::Constant, cx));
            v.set_tangent_mode(TangentMode::Mirrored);
            assert!(v.set_tangent(false, (0.1, 0.2), cx));
            let index = v.add_curve(
                CurveSeries::new("other", Curve::default(), guiraffe_core::ColorName::Red),
                cx,
            );
            v.set_curve_visible(index, false, cx);
            assert!(!v.curves()[index].visible);
            assert!(v.remove_selected(cx));
            v.fit_view(true, cx);
        })
    });
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| view.read(cx).curve().keys().len()), 2);
}

#[gpui_kit::test]
fn editing_apis_invalidate_only_the_editor(cx: &mut TestAppContext) {
    {
        let (view, renders, vcx) = mount(cx, dope);
        let before = renders.get();
        assert!(before > 0);
        vcx.update(|_, cx| {
            view.update(cx, |v, cx| {
                v.select_all(cx);
                v.nudge(2., cx);
            })
        });
        vcx.run_until_parked();
        assert_eq!(
            vcx.update(|_, cx| view.read(cx).channels()[1].keys()[0]),
            0.7
        );
        if gpui_kit::GPUI_FAST {
            assert_eq!(renders.get(), before, "dope API keeps shell");
        }
    }
    {
        let (view, renders, vcx) = mount(cx, curves);
        let before = renders.get();
        assert!(before > 0);
        vcx.update(|_, cx| {
            view.update(cx, |v, cx| {
                v.insert_at(1., cx);
            })
        });
        vcx.run_until_parked();
        assert_eq!(vcx.update(|_, cx| view.read(cx).curve().keys().len()), 3);
        if gpui_kit::GPUI_FAST {
            assert_eq!(renders.get(), before, "curve API keeps shell");
        }
    }
    {
        let (view, renders, vcx) = mount(cx, |cx| BarCurveView::new(BarCurve::new(32, 0., 1.), cx));
        let before = renders.get();
        assert!(before > 0);
        vcx.update(|_, cx| view.update(cx, |v, cx| v.draw_to(4., 0.8, cx)));
        vcx.run_until_parked();
        assert_eq!(vcx.update(|_, cx| view.read(cx).model().values()[4]), 0.8);
        if gpui_kit::GPUI_FAST {
            assert_eq!(renders.get(), before, "bar API keeps shell");
        }
    }
}

#[gpui_kit::test]
fn gantt_and_tracks_row_lock_buttons_toggle_admission(cx: &mut TestAppContext) {
    {
        let (view, _, vcx) = mount(cx, |cx| {
            GanttView::new(
                vec![Task {
                    id: 7,
                    name: "Task".into(),
                    not_before: 0.,
                    duration: 2.,
                    after: vec![],
                }],
                TimeFormat::Seconds,
                cx,
            )
        });
        let button = vcx
            .debug_bounds("task-lock-0")
            .expect("lock button")
            .center();
        click(vcx, button, false);
        assert!(vcx.update(|_, cx| view.read(cx).is_locked(7)));
        vcx.update(|_, cx| {
            view.update(cx, |v, _| {
                let x = v.axis().x(1.);
                assert!(!v.press(0, x));
            })
        });
        click(vcx, button, false);
        assert!(!vcx.update(|_, cx| view.read(cx).is_locked(7)));
    }
    {
        let mut track = Track::default();
        let id = track.insert(0., 2.).expect("clip");
        let (view, _, vcx) = mount(cx, |cx| {
            TracksView::new(vec![("Track".into(), track)], TimeFormat::Seconds, 3., cx)
        });
        let button = vcx
            .debug_bounds("track-lock-0")
            .expect("lock button")
            .center();
        click(vcx, button, false);
        assert!(vcx.update(|_, cx| view.read(cx).is_locked(0, id)));
        click(vcx, button, false);
        assert!(!vcx.update(|_, cx| view.read(cx).is_locked(0, id)));
    }
}

#[gpui_kit::test]
fn gantt_virtual_rows_scroll_and_hit_test_content_coordinates(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, |cx| {
        GanttView::new(
            (0..10_000)
                .map(|id| Task {
                    id,
                    name: format!("Task {id}"),
                    not_before: 0.0,
                    duration: 2.0,
                    after: Vec::new(),
                })
                .collect(),
            TimeFormat::Seconds,
            cx,
        )
    });
    assert!(vcx.debug_bounds("task-lock-0").is_some());
    assert!(vcx.debug_bounds("task-lock-9999").is_none());
    let body = vcx.debug_bounds("gantt-body").expect("Gantt body");
    vcx.simulate_event(gpui_kit::ScrollEvent {
        position: body.center(),
        delta: gpui_kit::ScrollDelta::Pixels(point(px(0.0), px(-320_000.0))),
        ..Default::default()
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("task-lock-0").is_none());
    assert!(vcx.debug_bounds("task-lock-9999").is_some());
    click(vcx, point(body.center().x, body.bottom() - px(16.0)), false);
    assert_eq!(vcx.update(|_, cx| view.read(cx).selected()), Some(9999));
}

#[gpui_kit::test]
fn dope_virtual_rows_scroll_without_instantiating_offscreen_controls(cx: &mut TestAppContext) {
    let (_, _, vcx) = mount(cx, |cx| {
        DopeSheet::new(
            (0..10_000)
                .map(|i| Channel::new(format!("Channel {i}"), vec![0.5]))
                .collect(),
            TimeFormat::Frames(24.),
            1.,
            cx,
        )
    });
    assert!(vcx.debug_bounds("channel-lock-0").is_some());
    assert!(vcx.debug_bounds("channel-lock-9999").is_none());
    let p = vcx.debug_bounds("dope-body").expect("body").center();
    vcx.simulate_event(gpui_kit::ScrollEvent {
        position: p,
        delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-300_000.))),
        ..Default::default()
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("channel-lock-0").is_none());
    assert!(vcx.debug_bounds("channel-lock-9999").is_some());
}
#[gpui_kit::test]
fn additive_box_selection_can_shrink_without_leaving_trailing_keys(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, dope);
    let b = dope_at(&view, vcx, 2, 1.3);
    click(vcx, b, false);
    let start = dope_at(&view, vcx, 1, 0.2);
    let wide = dope_at(&view, vcx, 1, 1.2);
    let narrow = dope_at(&view, vcx, 1, 0.6);
    vcx.simulate_mouse_down(
        start,
        MouseButton::Left,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    vcx.simulate_mouse_move(wide, Some(MouseButton::Left), Default::default());
    vcx.simulate_mouse_move(narrow, Some(MouseButton::Left), Default::default());
    vcx.simulate_mouse_up(narrow, MouseButton::Left, Default::default());
    vcx.run_until_parked();
    let selected = vcx.update(|_, cx| view.read(cx).model().selected.clone());
    assert_eq!(selected.len(), 2);
    assert!(selected.contains(&(1, 0.5_f64.to_bits())));
    assert!(selected.contains(&(2, 1.3_f64.to_bits())));
}

#[gpui_kit::test]
fn new_surfaces_have_roles_opt_in_focus_and_shared_ring(cx: &mut TestAppContext) {
    {
        let (view, _, vcx) = mount(cx, dope);
        let p = dope_at(&view, vcx, 1, 0.5);
        click(vcx, p, false);
        assert!(!vcx.update(|w, cx| view.read(cx).focus_handle(cx).is_focused(w)));
        vcx.update(|w, cx| {
            w.render_frame(cx);
            let node = w.find("dope-sheet");
            assert_eq!(node.role(), Some(Role::Tree));
            assert_eq!(node.label(), Some("Dope sheet"));
        });
        vcx.update(|_, cx| view.update(cx, |v, cx| v.set_focusable(true, cx)));
        let before = vcx.debug_bounds("dope-body").expect("body");
        vcx.update(|w, cx| {
            w.focus_next(cx);
            w.press("f13", cx);
        });
        vcx.run_until_parked();
        assert!(vcx.debug_bounds("focus-ring").is_some());
        assert_eq!(vcx.debug_bounds("dope-body"), Some(before));
    }
    {
        let (view, _, vcx) = mount(cx, curves);
        let p = curve_at(&view, vcx, 0.5, 0.5);
        click(vcx, p, false);
        assert!(!vcx.update(|w, cx| view.read(cx).focus_handle(cx).is_focused(w)));
        vcx.update(|w, cx| {
            w.render_frame(cx);
            let node = w.find("curve-body");
            assert_eq!(node.role(), Some(Role::Group));
            assert_eq!(node.label(), Some("Curve editor"));
        });
        vcx.update(|_, cx| view.update(cx, |v, cx| v.set_focusable(Some(true), cx)));
        let before = vcx.debug_bounds("curve-body").expect("body");
        vcx.update(|w, cx| {
            w.focus_next(cx);
            w.press("f13", cx);
        });
        vcx.run_until_parked();
        assert!(vcx.debug_bounds("focus-ring").is_some());
        assert_eq!(vcx.debug_bounds("curve-body"), Some(before));
    }
    {
        let (view, _, vcx) = mount(cx, |cx| BarCurveView::new(BarCurve::new(8, 0., 1.), cx));
        let p = vcx.debug_bounds("bar-curve").expect("bars").center();
        click(vcx, p, false);
        assert!(!vcx.update(|w, cx| view.read(cx).focus_handle(cx).is_focused(w)));
        vcx.update(|w, cx| {
            w.render_frame(cx);
            let node = w.find("bar-curve");
            assert_eq!(node.role(), Some(Role::Group));
            assert_eq!(node.label(), Some("Bar curve editor"));
        });
        vcx.update(|_, cx| view.update(cx, |v, cx| v.set_focusable(Some(true), cx)));
        let before = vcx.debug_bounds("bar-curve").expect("bars");
        vcx.update(|w, cx| {
            w.focus_next(cx);
            w.press("f13", cx);
        });
        vcx.run_until_parked();
        assert!(vcx.debug_bounds("focus-ring").is_some());
        assert_eq!(vcx.debug_bounds("bar-curve"), Some(before));
    }
}
#[gpui_kit::test]
fn dope_grab_offset_does_not_jump_on_first_move(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, |cx| {
        DopeSheet::new(
            vec![Channel::new("x", vec![0.5])],
            TimeFormat::Seconds,
            2.,
            cx,
        )
    });
    let b = vcx.debug_bounds("dope-body").expect("body");
    let (x, scale) = vcx.update(|_, cx| {
        let a = view.read(cx).axis();
        (a.x(0.5), a.scale)
    });
    let press = point(b.left() + px(x as f32 + 5.), b.top() + px(14.));
    let end = point(press.x + px(1.), press.y);
    drag(vcx, press, end);
    let time = vcx.update(|_, cx| view.read(cx).channels()[0].keys()[0]);
    assert!((time - 0.5 - scale).abs() < 1e-7);
}
#[gpui_kit::test]
fn box_anchor_stays_on_original_content_row_during_scroll(cx: &mut TestAppContext) {
    let (view, _, vcx) = mount(cx, |cx| {
        DopeSheet::new(
            (0..100)
                .map(|i| Channel::new(format!("row{i}"), vec![0.5]))
                .collect(),
            TimeFormat::Seconds,
            1.,
            cx,
        )
    });
    let a = dope_at(&view, vcx, 0, 0.1);
    let end = dope_at(&view, vcx, 1, 0.8);
    vcx.simulate_mouse_down(a, MouseButton::Left, Default::default());
    vcx.simulate_event(gpui_kit::ScrollEvent {
        position: a,
        delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-56.))),
        ..Default::default()
    });
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
    vcx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    vcx.run_until_parked();
    let selected = vcx.update(|_, cx| view.read(cx).model().selected.clone());
    assert_eq!(selected.len(), 4);
    assert!(selected.contains(&(0, 0.5_f64.to_bits())));
    assert!(selected.contains(&(3, 0.5_f64.to_bits())));
}
