use common::tick_from_parts;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use engine_core::{MomentumStrategy, Strategy, StrategyContext};

fn bench_strategy(c: &mut Criterion) {
    let strategy = MomentumStrategy::new(0.5, "SIM");
    let tick = tick_from_parts("ABC", 10.0, 100.0, 1, &[0; 16]);
    let snapshot = (&tick).into();
    let ctx = StrategyContext {
        tick: &tick,
        snapshot,
        mean: 10.0,
        momentum: 1.0,
    };

    c.bench_function("momentum_decision", |b| {
        b.iter_batched(|| ctx.clone(), |context| { let _ = strategy.on_event(&context); }, BatchSize::SmallInput)
    });
}

criterion_group!(benches, bench_strategy);
criterion_main!(benches);
