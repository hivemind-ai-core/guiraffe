//! spec/00 §12: Gantt and tracks views are keyboard-operable when focused (opt-in), a pointer
//! press preserves focus by default, items expose roles / labels / selected state, locked items use
//! disabled tokens, and focus / selection never move layout.
use gpui_kit::{
    AppContext, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    Role, Styled, TestAppContext, VisualTestContext, Window,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_core::focus_scope;
use guiraffe_test::layout::{assert_same_bounds, capture};
use guiraffe_timeline::{GanttView, Task, TimeFormat, Track, TracksView, tracks_view};

struct Host {
    input: Entity<InputState>,
    gantt: Entity<GanttView>,
    tracks: Entity<TracksView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().w(px(200.)).child(Input::new(&self.input)))
            .child(focus_scope(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .debug_selector(|| "gantt-box".into())
                            .h(px(140.))
                            .w(px(800.))
                            .child(self.gantt.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "tracks-box".into())
                            .h(px(140.))
                            .w(px(800.))
                            .child(self.tracks.clone()),
                    ),
            ))
    }
}

fn task(id: u32, nb: f64, d: f64) -> Task {
    Task {
        id,
        name: format!("t{id}"),
        not_before: nb,
        duration: d,
        after: vec![],
    }
}

fn mount(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        guiraffe_timeline::init(cx);
    });
    let (host, vcx) = cx.add_window_view(|window, cx| {
        let gantt = GanttView::new(
            vec![task(1, 0.0, 4.0), task(2, 2.0, 4.0)],
            TimeFormat::Seconds,
            cx,
        );
        gantt.update(cx, |g, _| g.set_key_step(Some(1.0)));
        let mut a = Track::default();
        a.insert(1.0, 2.0);
        a.insert(6.0, 1.0);
        let tracks = TracksView::new(vec![("A".into(), a)], TimeFormat::Seconds, 10.0, cx);
        tracks.update(cx, |t, _| t.set_snap(Some(0.5)));
        Host {
            input: cx.new(|cx| InputState::new(window, cx)),
            gantt,
            tracks,
        }
    });
    vcx.run_until_parked();
    vcx.update(|w, cx| w.render_frame(cx));
    vcx.run_until_parked();
    (host, vcx)
}

fn keyboard_focus_next(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.focus_next(cx);
        // Any keystroke marks the modality as keyboard, as a real Tab would.
        window.press("f13", cx);
    });
}

#[gpui_kit::test]
fn gantt_keyboard_moves_and_resizes_the_selected_task(cx: &mut TestAppContext) {
    let (host, vcx) = mount(cx);
    let gantt = host.read_with(vcx, |h, _| h.gantt.clone());
    let sels = ["gantt-box", "tracks-box"];
    let base = capture(vcx, &sels);
    let input = host.read_with(vcx, |h, cx| h.input.focus_handle(cx));
    vcx.update(|w, cx| w.focus(&input, cx));
    // Click a bar: selects it, keeps the input focused.
    vcx.update(|window, cx| {
        window.click(("gantt-bar", 0usize), cx);
        assert!(input.is_focused(window), "bar click stole focus");
    });
    assert_eq!(gantt.read_with(vcx, |g, _| g.selected()), Some(1));
    keyboard_focus_next(vcx);
    assert!(vcx.update(|w, cx| gantt.read(cx).focus_handle(cx).is_focused(w)));
    assert!(vcx.debug_bounds("focus-ring").is_some());
    assert_same_bounds(
        vcx,
        &sels,
        &base,
        "the Gantt view is focused and a bar selected",
    );
    let span = |vcx: &mut VisualTestContext| {
        gantt.read_with(vcx, |g, _| {
            let s = g.schedule().as_ref().ok().and_then(|m| m.get(&1)).copied();
            s.map(|s| (s.early_start, s.early_finish))
        })
    };
    vcx.simulate_keystrokes("right");
    assert_eq!(span(vcx), Some((1.0, 5.0)), "→ moves by one step");
    vcx.simulate_keystrokes("shift-right");
    assert_eq!(span(vcx), Some((1.0, 6.0)), "Shift+→ grows the end");
    vcx.simulate_keystrokes("alt-right");
    assert_eq!(span(vcx), Some((2.0, 6.0)), "Alt+→ moves the start");
    vcx.simulate_keystrokes("down");
    assert_eq!(gantt.read_with(vcx, |g, _| g.selected()), Some(2));
    // Locked tasks ignore keys.
    vcx.update(|_, cx| gantt.update(cx, |g, cx| g.set_locked(2, true, cx)));
    let before = gantt.read_with(vcx, |g, _| g.schedule().clone());
    vcx.simulate_keystrokes("right");
    assert_eq!(gantt.read_with(vcx, |g, _| g.schedule().clone()), before);
}

#[gpui_kit::test]
fn tracks_keyboard_moves_and_trims_the_selected_clip(cx: &mut TestAppContext) {
    let (host, vcx) = mount(cx);
    let tracks = host.read_with(vcx, |h, _| h.tracks.clone());
    vcx.update(|_, cx| tracks.update(cx, |t, cx| t.set_focusable(Some(true), cx)));
    let clip = |vcx: &mut VisualTestContext| {
        tracks.read_with(vcx, |t, _| {
            let c = &t.tracks()[0].1.clips()[0];
            (c.start, c.end())
        })
    };
    vcx.update(|w, cx| {
        w.render_frame(cx);
        w.click(tracks_view::clip_element_id(0, 1), cx);
        assert!(tracks.read(cx).focus_handle(cx).is_focused(w));
    });
    assert_eq!(tracks.read_with(vcx, |t, _| t.selected()), Some((0, 1)));
    vcx.simulate_keystrokes("right");
    assert_eq!(clip(vcx), (1.5, 3.5));
    vcx.simulate_keystrokes("shift-left");
    assert_eq!(clip(vcx), (1.5, 3.0));
    // The start cannot extend before the clip's source; trimming it in works.
    vcx.simulate_keystrokes("alt-right");
    assert_eq!(clip(vcx), (2.0, 3.0));
}

#[gpui_kit::test]
fn items_expose_roles_labels_and_selection(cx: &mut TestAppContext) {
    let (host, vcx) = mount(cx);
    let gantt = host.read_with(vcx, |h, _| h.gantt.clone());
    vcx.update(|_, cx| gantt.update(cx, |g, cx| g.select(Some(2), cx)));
    vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("gantt").role(), Some(Role::ListBox));
        let bar = window.find(("gantt-bar", 1usize));
        assert_eq!(bar.role(), Some(Role::ListBoxOption));
        assert_eq!(bar.selected(), Some(true));
        assert!(
            bar.label().is_some_and(|l| l.starts_with("t2:")),
            "{:?}",
            bar.label()
        );
        assert_eq!(window.find("tracks").role(), Some(Role::ListBox));
        let clip = window.find(tracks_view::clip_element_id(0, 1));
        assert_eq!(clip.role(), Some(Role::ListBoxOption));
        assert_eq!(window.find_all_by_role(Role::ListBoxOption).len(), 4);
    });
}
