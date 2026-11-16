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
main!(config = LibraryBenchmarkConfig::default().tool(Callgrind::default().soft_limits([(EventKind::Ir, 2.0)])); library_benchmark_groups = workloads);
