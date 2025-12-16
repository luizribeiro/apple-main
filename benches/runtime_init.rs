use apple_main::criterion::{criterion_group, Criterion};

fn benchmark_block_on(c: &mut Criterion) {
    c.bench_function("block_on_without_manual_init", |b| {
        b.iter(|| apple_main::block_on(async { 42 }))
    });
}

criterion_group!(benches, benchmark_block_on);
apple_main::criterion_main!(benches);
