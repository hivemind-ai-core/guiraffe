use iai_callgrind::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};

macro_rules! mounted_frames {
    ($($function:ident),+ $(,)?) => {
        $(
            #[library_benchmark(setup = guiraffe_bench::suites::$function)]
            fn $function(_: ()) {
                guiraffe_bench::suites::$function();
            }
        )+
        library_benchmark_group!(name = frames; benchmarks = $($function),+);
    };
}

mounted_frames!(
    datagrid_frame_100k,
    datagrid_frame_10m,
    code_frame_100k,
    code_frame_10m,
);

main!(
    config = LibraryBenchmarkConfig::default()
        .tool(Callgrind::default().soft_limits([(EventKind::Ir, 2.0)]));
    library_benchmark_groups = frames
);
