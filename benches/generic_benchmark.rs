use criterion::{criterion_group, criterion_main, Criterion};
use openworkers_runtime_deno::Worker;

openworkers_core::generate_worker_benchmarks!(Worker);

criterion_group!(benches, worker_benchmarks);
criterion_main!(benches);
