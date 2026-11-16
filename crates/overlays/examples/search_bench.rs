//! Pure benchmark: rustc --edition=2024 -O crates/overlays/examples/search_bench.rs -o /tmp/search-bench
#[path = "../src/search.rs"]
#[allow(dead_code)]
mod search;
use std::{hint::black_box, time::Instant};
fn main() {
    let commands: Vec<_> = (0..10_000)
        .map(|i| search::Command::new(format!("id-{i}"), format!("Open document {i:05}")))
        .collect();
    let prepared = Instant::now();
    let mut index = search::SearchIndex::new(&commands);
    println!("prepare_us={}", prepared.elapsed().as_micros());
    let queries = ["open42", "doc99", "od01", "save", "abc"];
    let mut samples = Vec::new();
    for _ in 0..20 {
        for query in queries {
            let start = Instant::now();
            black_box(index.search(black_box(query), 50));
            samples.push(start.elapsed().as_nanos());
        }
    }
    samples.sort_unstable();
    println!(
        "commands=10000 queries={} median_us={} p95_us={} min_us={} max_us={}",
        samples.len(),
        samples[samples.len() / 2] / 1000,
        samples[samples.len() * 95 / 100] / 1000,
        samples[0] / 1000,
        samples[samples.len() - 1] / 1000
    );
}
