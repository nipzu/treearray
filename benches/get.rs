use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use rand::{thread_rng, Rng};

use im::Vector;

use btreevec::BTreeVec;

fn bench_get(c: &mut Criterion) {
    let mut rng = thread_rng();
    let mut bvec = BTreeVec::<i32, 32, 64>::new();
    let mut vec = Vec::new();
    let mut im_vec = Vector::new();
    let size = 10_000;
    for x in 0..size {
        let i = rng.gen_range(0..=bvec.len());

        bvec.insert(i, x as i32);
        vec.insert(i, x as i32);
        im_vec.insert(i, x as i32);
    }

    c.bench_function("BVec<i32,63,63>::get (random, inbounds)", |b| {
        b.iter_batched(
            || rng.gen_range(0..size),
            |i| bvec.get(i),
            BatchSize::SmallInput,
        )
    });

    c.bench_function("Vec<i32>::get (random, inbounds)", |b| {
        b.iter_batched(
            || rng.gen_range(0..size),
            |i| vec.get(i),
            BatchSize::SmallInput,
        )
    });

    c.bench_function("im::Vector<i32>::get (random, inbounds)", |b| {
        b.iter_batched(
            || rng.gen_range(0..size),
            |i| im_vec.get(i),
            BatchSize::SmallInput,
        )
    });
}

fn bench_insert(c: &mut Criterion) {
    let mut rng = thread_rng();
    let mut bvec = BTreeVec::<i32, 32, 64>::new();
    let mut vec = Vec::new();
    let mut im_vec = Vector::new();
    let size = 10_000;
    for x in 0..size {
        let i = rng.gen_range(0..=bvec.len());

        bvec.insert(i, x as i32);
        vec.insert(i, x as i32);
        im_vec.insert(i, x as i32);
    }

    c.bench_function("BVec<i32,63,63>::insert (random)", |b| {
        b.iter_batched(
            || rng.gen_range(0..=size),
            |i| {
                bvec.insert(i, 0);
                bvec.remove(i);
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("Vec<i32>::insert (random)", |b| {
        b.iter_batched(
            || rng.gen_range(0..=size),
            |i| {
                vec.insert(i, 0);
                vec.remove(i);
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("im::Vector<i32>::insert (random)", |b| {
        b.iter_batched(
            || rng.gen_range(0..=size),
            |i| {
                im_vec.insert(i, 0);
                im_vec.remove(i)
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_get, bench_insert);
criterion_main!(benches);
