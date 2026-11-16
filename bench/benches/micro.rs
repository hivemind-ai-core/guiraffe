use criterion::{Criterion, criterion_group, criterion_main};
fn workloads(c: &mut Criterion) {
    for suite in guiraffe_bench::suites::registered() {
        c.bench_function(suite.name, |b| b.iter(suite.run));
    }
}
criterion_group!(benches, workloads);
criterion_main!(benches);
