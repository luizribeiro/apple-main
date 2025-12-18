#![feature(custom_test_frameworks)]
#![test_runner(apple_main::criterion_runner)]

use apple_main::criterion::Criterion;
use apple_main::criterion_macro::criterion;

#[criterion]
fn bench_block_on(c: &mut Criterion) {
    c.bench_function("block_on_noop", |b| {
        b.iter(|| apple_main::block_on(async { 42 }))
    });
}

#[criterion]
fn bench_on_main_sync(c: &mut Criterion) {
    c.bench_function("on_main_sync_noop", |b| {
        b.iter(|| apple_main::on_main_sync(|| 42))
    });
}
