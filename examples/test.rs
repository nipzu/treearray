use bvec::BVec;
use rand::{rngs::SmallRng, Rng, SeedableRng};

fn main() {
    let mut rng = SmallRng::seed_from_u64(21345);
    let mut v = BVec::<u32>::new();
    for x in 0..1_000_000 {
        let i = rng.gen_range(0..=v.len());
        v.insert(i, x);
    }

    let mut res = 0;
    for _ in 0..10_000_000 {
        let i = rng.gen_range(0..v.len());
        res ^= v.get(i).unwrap();
    }
    println!("{res}");
    std::mem::forget(v);
}
