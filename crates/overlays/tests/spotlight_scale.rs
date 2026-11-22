//! Scale gates for the command bar's pure model: 10k actions mount in ≤ 50 ms and filter in
//! ≤ 8 ms per keystroke, and a keystroke allocates nothing once buffers are warm.
//! Timings are asserted in optimised builds (`cargo test --release`); workspace crates are
//! unoptimised in debug, where only the allocation gates apply and timings are printed.
#![allow(unsafe_code)]
use guiraffe_overlays::spotlight::{RowHeights, SpotlightAction, SpotlightModel};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    time::{Duration, Instant},
};

struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}
// SAFETY: forwards to the system allocator; only counts calls on the current thread.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

const GROUPS: [&str; 6] = ["File", "Edit", "View", "Git", "Debug", "Window"];

fn actions(n: usize) -> Vec<SpotlightAction> {
    (0..n)
        .map(|i| {
            let mut a = SpotlightAction::new(
                format!("cmd.{i}"),
                format!(
                    "{} document {i:05} settings",
                    ["Open", "Close", "Save", "Rename"][i % 4]
                ),
            )
            .group(GROUPS[i % GROUPS.len()])
            .keywords("workspace project");
            if i % 3 == 0 {
                a = a.description(format!("Applies to buffer {i}"));
            }
            if i % 5 == 0 {
                a = a.shortcut("Ctrl+K Ctrl+S");
            }
            a
        })
        .collect()
}

fn within(elapsed: Duration, budget: Duration) -> bool {
    cfg!(debug_assertions) || elapsed <= budget
}

#[test]
fn ten_thousand_actions_mount_and_filter_within_budget() {
    let data = actions(10_000);
    let heights = RowHeights {
        header: 30.,
        action: 42.,
        action_with_description: 52.,
    };
    // Timings take the best of a few repetitions: the gate is the code's cost, not the
    // scheduling noise of a loaded CI machine.
    let mut mount = Duration::MAX;
    let mut model = SpotlightModel::new(heights);
    for _ in 0..5 {
        let copy = data.clone();
        let start = Instant::now();
        let mut fresh = SpotlightModel::new(heights);
        fresh.set_actions(copy);
        mount = mount.min(start.elapsed());
        model = fresh;
    }
    assert_eq!(model.result_count(), 10_000);
    assert!(
        within(mount, Duration::from_millis(50)),
        "mount took {mount:?}"
    );

    // Warm the buffers with one full pass of the queries used below.
    let queries = [
        "o", "op", "ope", "open", "open d", "open doc", "s", "sa", "sav", "save 9",
    ];
    for q in queries {
        model.set_query(q);
    }
    model.set_query("");

    let mut best = [Duration::MAX; 10];
    let before = allocations();
    for _ in 0..5 {
        for (q, best) in queries.iter().zip(best.iter_mut()) {
            let t = Instant::now();
            model.set_query(q);
            *best = (*best).min(t.elapsed());
            assert!(model.result_count() > 0, "{q}");
        }
        model.set_query("");
    }
    let allocated = allocations() - before;
    let worst = best.into_iter().max().unwrap_or_default();
    assert!(
        within(worst, Duration::from_millis(8)),
        "slowest keystroke {worst:?}"
    );
    assert_eq!(allocated, 0, "keystrokes allocated {allocated} times");
    eprintln!("10k: mount {mount:?}, slowest keystroke {worst:?}");
}

#[test]
fn navigation_and_visible_range_do_not_allocate() {
    let mut model = SpotlightModel::new(RowHeights {
        header: 30.,
        action: 42.,
        action_with_description: 52.,
    });
    model.set_actions(actions(10_000));
    model.set_query("doc");
    let before = allocations();
    for _ in 0..100 {
        model.move_by(1);
        model.cycle_section(true);
        let row = model.selected_row().unwrap_or(0);
        let range = model.visible_rows(model.row_top(row), 400.);
        assert!(range.len() < 20);
    }
    assert_eq!(allocations() - before, 0);
}
