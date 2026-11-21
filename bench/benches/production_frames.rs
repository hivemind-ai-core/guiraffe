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

macro_rules! warm_repaints {
    ($($function:ident),+ $(,)?) => {
        $(
            #[library_benchmark]
            fn $function() {
                guiraffe_bench::suites::$function();
            }
        )+
        library_benchmark_group!(name = warm; benchmarks = $($function),+);
    };
}
mounted_frames!(
    datagrid_frame_100k,
    datagrid_frame_10m,
    code_frame_100k,
    code_frame_10m,
);

struct ExpansionSource(Vec<u64>);

impl guiraffe_tree::TreeSource for ExpansionSource {
    fn roots(&self) -> &[u64] {
        &self.0
    }

    fn children(&self, id: u64) -> guiraffe_tree::Children<'_> {
        guiraffe_tree::Children::Ready(if id == 0 { &[u64::MAX] } else { &[] })
    }

    fn label(&self, _: u64) -> &str {
        "row"
    }
}

fn prepare_expansion(size: u64) -> (guiraffe_tree::TreeModel, ExpansionSource) {
    let source = ExpansionSource((0..size).collect());
    let mut tree = guiraffe_tree::TreeModel::new(&source);
    tree.set_expanded(0, true, &source);
    tree.set_expanded(0, false, &source);
    (tree, source)
}

#[library_benchmark]
#[bench::rows_10k(args = (10_000), setup = prepare_expansion)]
#[bench::rows_1m(args = (1_000_000), setup = prepare_expansion)]
fn tree_local_expand_and_collapse(
    (mut tree, source): (guiraffe_tree::TreeModel, ExpansionSource),
) -> (guiraffe_tree::TreeModel, ExpansionSource) {
    tree.set_expanded(0, true, &source);
    tree.set_expanded(0, false, &source);
    (tree, source)
}

library_benchmark_group!(name = tree_updates; benchmarks = tree_local_expand_and_collapse);

warm_repaints!(warm_components, warm_inputs, warm_layout, warm_overlays,);

main!(
    config = LibraryBenchmarkConfig::default()
        .tool(Callgrind::default().soft_limits([(EventKind::Ir, 2.0)]));
    library_benchmark_groups = frames, tree_updates, warm
);
