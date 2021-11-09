#![feature(test)]

extern crate test;

use self::test::{black_box, Bencher};
use btreevec::BTreeVec;

#[bench]
fn bench_push_back_3_3(b: &mut Bencher) {
    let mut bvec = BTreeVec::<i32, 3, 3>::new();
    for x in 0..10_000 {
        bvec.push_back(x);
    }

    b.iter(|| {
        bvec.push_back(black_box(0));
    });
}

#[bench]
fn bench_push_back_31_31(b: &mut Bencher) {
    let mut bvec = BTreeVec::<i32, 31, 31>::new();
    for x in 0..10_000 {
        bvec.push_back(x);
    }

    b.iter(|| {
        bvec.push_back(black_box(0));
    });
}

#[bench]
fn bench_push_back_vec(b: &mut Bencher) {
    let mut vec = Vec::<i32>::new();
    for x in 0..10_000 {
        vec.push(x);
    }

    b.iter(|| {
        vec.push(black_box(0));
    });
}

#[bench]
fn bench_push_front_3_3(b: &mut Bencher) {
    let mut bvec = BTreeVec::<i32, 3, 3>::new();
    for x in 0..10_000 {
        bvec.push_front(x);
    }

    b.iter(|| {
        bvec.push_front(black_box(0));
    });
}

#[bench]
fn bench_push_front_31_31(b: &mut Bencher) {
    let mut bvec = BTreeVec::<i32, 31, 31>::new();
    for x in 0..10_000 {
        bvec.push_front(x);
    }

    b.iter(|| {
        bvec.push_front(black_box(0));
    });
}

#[bench]
fn bench_push_front_63_127(b: &mut Bencher) {
    let mut bvec = BTreeVec::<i32, 63, 127>::new();
    for x in 0..10_000 {
        bvec.push_front(x);
    }

    b.iter(|| {
        bvec.push_front(black_box(0));
    });
}

#[bench]
fn bench_push_front_15_15(b: &mut Bencher) {
    let mut bvec = BTreeVec::<i32, 15, 15>::new();
    for x in 0..10_000 {
        bvec.push_front(x);
    }

    b.iter(|| {
        bvec.push_front(black_box(0));
    });
}

#[bench]
fn bench_push_front_vec(b: &mut Bencher) {
    let mut vec = Vec::<i32>::new();
    for x in 0..10_000 {
        vec.push(x);
    }

    b.iter(|| {
        vec.insert(0, black_box(0));
    });
}
