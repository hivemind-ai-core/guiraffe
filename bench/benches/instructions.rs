use iai_callgrind::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
macro_rules! define_instruction {
    (data_ui_instantiate) => {
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_data_ui)]
        fn data_ui_instantiate(_: ()) {
            guiraffe_bench::suites::data_ui_instantiate();
        }
    };
    (data_ui_patch) => {
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_data_ui)]
        fn data_ui_patch(_: ()) {
            guiraffe_bench::suites::data_ui_patch();
        }
    };
    (data_ui_splice) => {
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_data_ui)]
        fn data_ui_splice(_: ()) {
            guiraffe_bench::suites::data_ui_splice();
        }
    };
    (data_ui_structural_splice) => {
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_data_ui)]
        fn data_ui_structural_splice(_: ()) {
            guiraffe_bench::suites::data_ui_structural_splice();
        }
    };
    (chart_line_million) => {
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_chart_line)]
        fn chart_line_million(_: ()) {
            guiraffe_bench::suites::chart_line_million();
        }
    };
    (chart_scatter_hundred_thousand) => {
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_chart_scatter)]
        fn chart_scatter_hundred_thousand(_: ()) {
            guiraffe_bench::suites::chart_scatter_hundred_thousand();
        }
    };
    ($function:ident) => {
        #[library_benchmark]
        fn $function() {
            guiraffe_bench::suites::$function();
        }
    };
}
macro_rules! register_instructions {
    ($(($function:ident, $name:literal),)*) => {
        $(
            define_instruction!($function);
        )*
        library_benchmark_group!(name = workloads; benchmarks = $($function),*);
    };
}
guiraffe_bench::with_suites!(register_instructions);

fn registered_controls() -> guiraffe_bench::protocol::Fixture {
    guiraffe_bench::protocol::Fixture::new(true).expect("mount protocol fixture")
}

// One registered toggle dispatched through the window bus, excluding path lookup.
// The bench profile measured 2,111 Ir on x86-64 at the 0.3 phase 1 core. The
// hard ceiling applies even before a base revision contains this suite.
#[library_benchmark(setup = registered_controls, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::default().hard_limits([(EventKind::Ir, 3_000)]).soft_limits([(EventKind::Ir, 2.0)])
))]
fn registered_dispatch(
    mut fixture: guiraffe_bench::protocol::Fixture,
) -> guiraffe_bench::protocol::Fixture {
    fixture.dispatch_once().expect("dispatch protocol action");
    fixture
}
library_benchmark_group!(name = protocol; benchmarks = registered_dispatch);

fn ribbon_fixture() -> guiraffe_bench::ribbon::RibbonFixture {
    guiraffe_bench::ribbon::RibbonFixture::new().expect("build 2k-command ribbon")
}

// 2,000 commands in 200 categories (command-ribbon.md §5). Hard ceilings sit about 1.5x over
// the counts measured on 2026-11-25; the 2% soft limit tracks regressions against a baseline.
#[library_benchmark(setup = ribbon_fixture, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::default().hard_limits([(EventKind::Ir, RIBBON_OPEN_IR)]).soft_limits([(EventKind::Ir, 2.0)])
))]
fn ribbon_open(
    fixture: guiraffe_bench::ribbon::RibbonFixture,
) -> guiraffe_bench::ribbon::RibbonFixture {
    // Returned, so dropping 2,000 commands is not measured.
    std::hint::black_box(fixture.open());
    fixture
}
#[library_benchmark(setup = ribbon_fixture, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::default().hard_limits([(EventKind::Ir, RIBBON_NAVIGATE_IR)]).soft_limits([(EventKind::Ir, 2.0)])
))]
fn ribbon_navigate(
    fixture: guiraffe_bench::ribbon::RibbonFixture,
) -> guiraffe_bench::ribbon::RibbonFixture {
    // Returned, so dropping 2,000 commands is not measured.
    std::hint::black_box(fixture.navigate().expect("navigate"));
    fixture
}
#[library_benchmark(setup = ribbon_fixture, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::default().hard_limits([(EventKind::Ir, RIBBON_FILTER_IR)]).soft_limits([(EventKind::Ir, 2.0)])
))]
fn ribbon_filter(
    fixture: guiraffe_bench::ribbon::RibbonFixture,
) -> guiraffe_bench::ribbon::RibbonFixture {
    // Returned, so dropping 2,000 commands is not measured.
    std::hint::black_box(fixture.filter().expect("filter"));
    fixture
}
const RIBBON_OPEN_IR: u64 = 36_000; // measured 24,423
const RIBBON_NAVIGATE_IR: u64 = 68_000; // measured 45,119
const RIBBON_FILTER_IR: u64 = 64_000; // measured 42,079
library_benchmark_group!(name = ribbon; benchmarks = ribbon_open, ribbon_navigate, ribbon_filter);
main!(config = LibraryBenchmarkConfig::default().tool(Callgrind::default().soft_limits([(EventKind::Ir, 2.0)])); library_benchmark_groups = workloads, protocol, ribbon);
