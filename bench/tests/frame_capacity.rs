use gpui_kit::{
    Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, TestAppContext, Window, div,
};

struct KeyedRows {
    focus: Vec<FocusHandle>,
}

impl Render for KeyedRows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().children(
            self.focus
                .iter()
                .enumerate()
                .map(|(index, focus)| div().id(index).track_focus(focus).tab_stop(true)),
        )
    }
}

#[gpui_kit::test]
#[ignore = "100k keyed rows, run with --ignored --nocapture"]
fn alternating_frames_retain_peak_storage(cx: &mut TestAppContext) {
    let (rows, mut window) = cx.add_window_view(|_, _| KeyedRows { focus: Vec::new() });
    let mut measure = |count| {
        window.update(|_, cx| {
            rows.update(cx, |rows, cx| {
                rows.focus.resize_with(count, || cx.focus_handle());
                cx.notify();
            });
        });
        window.update(|window, cx| {
            for _ in 0..2 {
                window.refresh();
                window.draw(cx).clear(cx);
            }
            window.test_frame_storage_capacity_bytes()
        })
    };
    let initial = measure(100);
    println!(
        "rendered/next bytes per frame [element states, accesses, tab history, tab index, tab order]"
    );
    println!("initial 100: {initial:?}");
    for peak in [10_000, 100_000] {
        println!("peak {peak}: {:?}", measure(peak));
        let retained = measure(100);
        println!("100 after {peak}: {retained:?}");
        for (frame, baseline) in retained.into_iter().zip(initial) {
            assert!(
                frame
                    .into_iter()
                    .zip(baseline)
                    .all(|(size, old)| size > old)
            );
        }
    }
}
