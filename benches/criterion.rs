use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

use pprof::criterion::{Output, PProfProfiler};

use rand::{rngs::StdRng, Rng, SeedableRng};

use bvec::BVec;

fn bench_get_bvec(c: &mut Criterion) {
    let mut rng = StdRng::from_seed([0; 32]);

    for size in [1_000, 10_000, 100_000, 1_000_000, 10_000_000] {
        let mut bvec = BVec::<i32>::new();

        for x in 0..size as i32 {
            let i = rng.gen_range(0..=bvec.len());
            bvec.insert(i, x);
        }

        c.bench_with_input(
            BenchmarkId::new("BVec<i32>::get (random)", size),
            &size,
            |b, &s| {
                b.iter_batched(
                    || rng.gen_range(0..s),
                    |i| bvec.get(i),
                    BatchSize::SmallInput,
                )
            },
        );
    }
}

fn bench_get_vec(c: &mut Criterion) {
    let mut rng = StdRng::from_seed([0; 32]);

    for size in [1_000, 10_000, 100_000, 1_000_000] {
        let mut vec = Vec::new();

        for x in 0..size as i32 {
            vec.push(x);
        }

        c.bench_with_input(
            BenchmarkId::new("std::Vec<i32>::get (random)", size),
            &size,
            |b, &s| {
                b.iter_batched(
                    || rng.gen_range(0..s),
                    |i| vec.get(i),
                    BatchSize::SmallInput,
                )
            },
        );
    }
}

fn bench_get_im_vec(c: &mut Criterion) {
    let mut rng = StdRng::from_seed([0; 32]);

    for size in [1_000, 10_000, 100_000] {
        let mut im_vec = im::Vector::new();

        for x in 0..size as i32 {
            let i = rng.gen_range(0..=im_vec.len());
            im_vec.insert(i, x);
        }

        c.bench_with_input(
            BenchmarkId::new("im::Vector<i32>::get (random)", size),
            &size,
            |b, &s| {
                b.iter_batched(
                    || rng.gen_range(0..s),
                    |i| im_vec.get(i),
                    BatchSize::SmallInput,
                )
            },
        );
    }
}

fn bench_insert(c: &mut Criterion) {
    let mut rng = StdRng::from_seed([0; 32]);

    for size in [1_000, 10_000, 100_000, 1_000_000] {
        let mut bvec = BVec::<i32>::new();
        let mut vec = Vec::new();

        for x in 0..size as i32 {
            let i = rng.gen_range(0..=bvec.len());

            bvec.insert(i, x);
            vec.push(x);
        }

        c.bench_with_input(
            BenchmarkId::new("BVec<i32>::insert_remove (random)", size),
            &size,
            |b, &s| {
                b.iter_batched(
                    || rng.gen_range(0..=s),
                    |i| {
                        let mut cursor = bvec.cursor_at_mut(i);
                        cursor.insert(0);
                        cursor.remove();
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        c.bench_with_input(
            BenchmarkId::new("std::Vec<i32>::insert_remove (random)", size),
            &size,
            |b, &s| {
                b.iter_batched(
                    || rng.gen_range(0..=s),
                    |i| {
                        vec.insert(i, 0);
                        vec.remove(i);
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(500).with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = bench_get_bvec, bench_get_vec, bench_get_im_vec, bench_insert
);
criterion_main!(benches);
