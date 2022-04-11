use criterion::{black_box, criterion_group, criterion_main, Criterion};

use btreevec::BTreeVec;

fn bench_get(c: &mut Criterion) {
    let mut bvec = BTreeVec::<i32, 63, 20_000>::new();
    let n = 10_000;
    for i in 0..n {
        bvec.push_back(i);
    }

    c.bench_function("get <4, 4>", |b| b.iter(|| bvec.get(black_box(4_321))));
}

criterion_group!(benches, bench_get);
criterion_main!(benches);
