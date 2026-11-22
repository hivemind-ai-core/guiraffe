use std::{io, sync::Arc, time::Instant};

use gpui_kit::{TestAppContext, px, size};
use guiraffe_sheet::{BuiltinEngine, Cell, CellPos, Recalculator, SheetModel, SheetState};
use guiraffe_test::alloc::{CountingAllocator, Counts, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const ROWS: usize = 1_000_000;
const COLS: usize = 1_000;
const FORMULAS: usize = 100_000;

fn submit_formula_chain(count: usize, finish: bool) -> io::Result<(Counts, std::time::Duration)> {
    let mut worker = Recalculator::new(ROWS, COLS, Arc::new(BuiltinEngine))?;
    let batch = (1..=count)
        .map(|row| (CellPos::new(row, 0), Arc::from(format!("=A{row}+1"))))
        .collect();
    let start = Instant::now();
    let (revision, counts) = count_allocations(|| worker.submit(batch));
    let elapsed = start.elapsed();
    assert!(revision.is_some());
    if finish {
        assert_eq!(worker.recv().map(|result| result.revision), revision);
    }
    Ok((counts, elapsed))
}

#[test]
fn initial_submit_allocations_are_independent_of_batch_size() -> io::Result<()> {
    // Initialize the caller thread's mailbox machinery outside the comparison.
    submit_formula_chain(1, false)?;
    let (small, _) = submit_formula_chain(1_000, false)?;
    let (large, _) = submit_formula_chain(FORMULAS, false)?;
    assert_eq!(small, large);
    Ok(())
}

#[gpui_kit::test]
#[ignore = "release-mode scale probe, run with --ignored --nocapture"]
fn sheet_submission_and_mount_probe(cx: &mut TestAppContext) {
    let (counts, submit) = submit_formula_chain(FORMULAS, true).expect("submit formula chain");
    println!("submit {FORMULAS} formulas: {submit:?}, {counts:?}");

    cx.update(guiraffe_core::init);
    let started = Instant::now();
    let (_, window) = cx.add_window_view(|_, cx| {
        let cells =
            (1..=FORMULAS).map(|row| (CellPos::new(row, 0), Cell::new(format!("=A{row}+1"))));
        SheetState::new(SheetModel::from_cells(ROWS, COLS, cells), cx)
    });
    window.simulate_resize(size(px(1280.), px(760.)));
    window.update(|window, cx| window.draw(cx).clear(cx));
    println!(
        "mount {ROWS} x {COLS}, {FORMULAS} formulas: {:?}",
        started.elapsed()
    );
}
