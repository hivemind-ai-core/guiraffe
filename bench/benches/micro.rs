use criterion::{Criterion, criterion_group, criterion_main};
fn workloads(c: &mut Criterion) {
    for suite in guiraffe_bench::suites::registered() {
        if suite.name.starts_with("data-ui-") {
            guiraffe_bench::suites::prepare_data_ui();
        }
        c.bench_function(suite.name, |b| b.iter(suite.run));
    }
}
criterion_group!(benches, workloads);
criterion_main!(benches);
