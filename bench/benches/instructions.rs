use iai_callgrind::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
macro_rules! define_instruction {
    (data_ui_instantiate) => {
        // Same binary, runs measured 2,299 to 2,663 Ir. Every gpui and guiraffe function
        // counted identically; the difference is glibc malloc/calloc slow paths, whose
        // heap state after setup depends on how GPUI's background threads were scheduled.
        // A 20 percent tolerance covers that spread; allocation traffic stays bounded in
        // bench/tests/allocator.rs.
        #[library_benchmark(setup = guiraffe_bench::suites::prepare_data_ui, config = LibraryBenchmarkConfig::default().tool(
            Callgrind::default().soft_limits([(EventKind::Ir, 20.0)])
        ))]
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
// The bench profile measured 2,121 Ir on x86-64 at the 0.3 phase 1 core. The
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
// Eight toggles through `dispatch_batch`, outcomes returned inline. Measured
// 6,407 Ir in the bench profile.
#[library_benchmark(setup = registered_controls, config = LibraryBenchmarkConfig::default().tool(
    Callgrind::default().hard_limits([(EventKind::Ir, 8_000)]).soft_limits([(EventKind::Ir, 2.0)])
))]
fn registered_batch(
    mut fixture: guiraffe_bench::protocol::Fixture,
) -> guiraffe_bench::protocol::Fixture {
    fixture
        .dispatch_batch_once()
        .expect("dispatch protocol batch");
    fixture
}
library_benchmark_group!(name = protocol; benchmarks = registered_dispatch, registered_batch);
main!(config = LibraryBenchmarkConfig::default().tool(Callgrind::default().soft_limits([(EventKind::Ir, 2.0)])); library_benchmark_groups = workloads, protocol);
