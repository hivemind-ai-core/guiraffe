use gpui_kit::{
    Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, Window, div,
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

/// Draws `rows` keyed rows, `frames` times, and returns the bytes the
/// rendered and next frames hold for [element states, accesses, tab history,
/// tab index, tab order].
fn draw_rows(
    rows: &gpui_kit::Entity<KeyedRows>,
    window: &mut gpui_kit::VisualTestContext,
    count: usize,
    frames: usize,
) -> [[usize; 5]; 2] {
    window.update(|_, cx| {
        rows.update(cx, |rows, cx| {
            rows.focus.resize_with(count, || cx.focus_handle());
            rows.focus.truncate(count);
            cx.notify();
        });
    });
    window.update(|window, cx| {
        for _ in 0..frames {
            window.refresh();
            window.draw(cx).clear(cx);
            window.focus_next(cx);
        }
        window.test_frame_storage_capacity_bytes()
    })
}

/// After a large frame, the per-frame buffers keep their capacity while
/// frames stay large or alternate, and give it back once frames have stayed
/// small for a while (see gpui-fast's `fast::high_water`).
fn peak_storage_is_given_back(cx: &mut gpui_kit::TestAppContext, peaks: &[usize]) {
    let (rows, window) = cx.add_window_view(|_, _| KeyedRows { focus: Vec::new() });
    let initial = draw_rows(&rows, window, 100, 2);
    println!(
        "rendered/next bytes per frame [element states, accesses, tab history, tab index, tab order]"
    );
    println!("initial 100: {initial:?}");
    for &peak in peaks {
        println!("peak {peak}: {:?}", draw_rows(&rows, window, peak, 2));
        let held = draw_rows(&rows, window, 100, 2);
        println!("100 just after {peak}: {held:?}");
        let trimmed = draw_rows(&rows, window, 100, 12);
        println!("100 a while after {peak}: {trimmed:?}");
        for (frame, baseline) in trimmed.into_iter().zip(initial) {
            for (size, old) in frame[..3].iter().zip(&baseline[..3]) {
                // What 100 rows need, or the floor under which nothing is trimmed.
                assert!(
                    *size <= (*old).max(256 * 1024),
                    "{peak} rows' storage was kept: {trimmed:?} against {initial:?}"
                );
            }
        }
    }
}

#[gpui_kit::test]
fn frame_storage_shrinks_back_after_10k_rows(cx: &mut gpui_kit::TestAppContext) {
    if gpui_kit::GPUI_FAST {
        peak_storage_is_given_back(cx, &[10_000]);
    }
}

#[gpui_kit::test]
#[ignore = "100k keyed rows, run with --ignored --nocapture"]
fn frame_storage_shrinks_back_after_100k_rows(cx: &mut gpui_kit::TestAppContext) {
    if gpui_kit::GPUI_FAST {
        peak_storage_is_given_back(cx, &[10_000, 100_000]);
    }
}
