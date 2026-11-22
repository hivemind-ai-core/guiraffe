//! Opt-in warm CPU draw probe for the mounted soak fixtures.
#![allow(unsafe_code)]

use backtrace::Backtrace;
use gpui_kit::{App, Window};
use guiraffe_bench::allocator::{CountingAllocator, Counts, count_allocations};
use std::{
    alloc::{GlobalAlloc, Layout},
    cell::RefCell,
    collections::BTreeMap,
    time::Instant,
};

type AllocationStacks = Vec<(usize, Backtrace)>;

thread_local! {
    static STACKS: RefCell<Option<AllocationStacks>> = const { RefCell::new(None) };
}

struct StackAllocator;

#[global_allocator]
static ALLOCATOR: StackAllocator = StackAllocator;

// SAFETY: Delegate the complete allocation contract to CountingAllocator.
// The capture hook only observes successful operations and skips reentrant calls.
unsafe impl GlobalAlloc for StackAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { CountingAllocator.alloc(layout) };
        observe(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { CountingAllocator.alloc_zeroed(layout) };
        observe(pointer, layout.size());
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { CountingAllocator.realloc(pointer, layout, size) };
        observe(pointer, size);
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { CountingAllocator.dealloc(pointer, layout) };
    }
}

fn observe(pointer: *mut u8, size: usize) {
    if !pointer.is_null() {
        let _ = STACKS.try_with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut()
                && let Some(stacks) = slot.as_mut()
            {
                stacks.push((size, Backtrace::new_unresolved()));
            }
        });
    }
}

pub fn enabled() -> bool {
    #[cfg(all(test, target_os = "linux"))]
    if test_items().is_some() {
        return true;
    }
    std::env::var_os("SOAK_WARM_REPAINT").is_some()
}

#[cfg(all(test, target_os = "linux"))]
#[derive(Clone, Copy)]
struct TestCase {
    items: usize,
    max_allocations: u64,
    measured: bool,
}

#[cfg(all(test, target_os = "linux"))]
thread_local! {
    static TEST_CASE: std::cell::Cell<Option<TestCase>> = const { std::cell::Cell::new(None) };
}

#[cfg(all(test, target_os = "linux"))]
pub fn test_items() -> Option<usize> {
    TEST_CASE.get().map(|case| case.items)
}

#[cfg(all(test, target_os = "linux"))]
fn verify(window: &mut Window, cx: &mut App, mut case: TestCase) -> super::Result<()> {
    for frame in 0..8 {
        let (_, counts) = count_allocations(|| draw(window, cx, false))?;
        if counts.allocations > case.max_allocations {
            if std::env::var_os("SOAK_ALLOC_STACKS").is_some() {
                print_frame_sites(window, cx, false);
            }
            return Err(format!(
                "retained frame {frame} exceeded {} allocations: {counts:?}",
                case.max_allocations
            )
            .into());
        }
    }
    case.measured = true;
    TEST_CASE.set(Some(case));
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
pub fn test_fixture(component: &str, items: usize, max_allocations: u64) -> super::Result<()> {
    TEST_CASE.set(Some(TestCase {
        items,
        max_allocations,
        measured: false,
    }));
    let result = super::run_component(component, true, false);
    let measured = TEST_CASE.take().is_some_and(|case| case.measured);
    result?;
    if !measured {
        return Err(format!("{component} never drew a retained frame").into());
    }
    Ok(())
}

#[inline(never)]
fn draw(window: &mut Window, cx: &mut App, invalidate: bool) {
    if invalidate {
        window.refresh();
    }
    window.draw(cx).clear(cx);
}

pub fn measure(window: &mut Window, cx: &mut App) -> super::Result<()> {
    #[cfg(all(test, target_os = "linux"))]
    if let Some(case) = TEST_CASE.get() {
        for _ in 0..8 {
            draw(window, cx, false);
        }
        return verify(window, cx, case);
    }

    let frames = std::env::var("SOAK_FRAMES")
        .ok()
        .map_or(Ok(1), |value| value.parse::<usize>())?
        .max(1);
    let invalidate = std::env::var_os("SOAK_INVALIDATE_WINDOW").is_some();
    for _ in 0..8 {
        draw(window, cx, invalidate);
    }
    let started = Instant::now();
    let (_, counts) = count_allocations(|| {
        for _ in 0..frames {
            draw(window, cx, invalidate);
        }
    })?;
    println!("warm repaint over {frames} draws: {counts:?}");
    println!("probe_elapsed_us={}", started.elapsed().as_micros());
    if std::env::var_os("SOAK_ALLOC_STACKS").is_some() {
        print_frame_sites(window, cx, invalidate);
    }
    Ok(())
}

fn print_frame_sites(window: &mut Window, cx: &mut App, invalidate: bool) {
    STACKS.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    draw(window, cx, invalidate);
    let stacks = STACKS.with(|slot| slot.borrow_mut().take().unwrap_or_default());
    print_sites(stacks);
}

fn print_sites(stacks: AllocationStacks) {
    let mut sites = BTreeMap::<String, Counts>::new();
    for (bytes, mut stack) in stacks {
        stack.resolve();
        let site = stack
            .frames()
            .iter()
            .flat_map(|frame| frame.symbols())
            .filter_map(|symbol| symbol.name().map(|name| (symbol, format!("{name:#}"))))
            .find(|(symbol, name)| {
                ![
                    "backtrace::",
                    "soak::warm_repaint::",
                    "guiraffe_bench::allocator::",
                    "std::",
                    "alloc::",
                    "core::",
                    "<std::",
                    "<alloc::",
                    "<core::",
                    "__rust",
                    "__rg_",
                    "__rdl_",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
                    && !name.contains("StackAllocator")
                    && !name.contains("CountingAllocator")
                    && name != "trace"
                    && !symbol.filename().is_some_and(|path| {
                        let path = path.to_string_lossy();
                        path.contains("/backtrace-")
                            || path.contains("/library/alloc/")
                            || path.contains("/library/core/")
                            || path.contains("/library/std/")
                            || path.ends_with("/warm_repaint.rs")
                            || path.ends_with("/bench/src/allocator.rs")
                    })
            });
        let key = site.map_or_else(
            || "<unresolved>".to_owned(),
            |(symbol, name)| {
                if let Some(path) = symbol.filename() {
                    let path = path.to_string_lossy();
                    let vendor_path = path
                        .strip_prefix("vendor/")
                        .or_else(|| path.rsplit_once("/vendor/").map(|(_, path)| path));
                    if let Some(vendor_crate) = vendor_path.and_then(|path| path.split('/').next())
                    {
                        return format!("{vendor_crate} {name}");
                    }
                }
                name
            },
        );
        let group = sites.entry(key).or_default();
        group.allocations += 1;
        group.bytes += bytes as u64;
    }
    let mut sites: Vec<_> = sites.into_iter().collect();
    sites.sort_by_key(|(_, count)| std::cmp::Reverse(count.allocations));
    for (site, counts) in sites {
        println!("{:>6} {:>10} {site}", counts.allocations, counts.bytes);
    }
}
