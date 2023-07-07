use bvec::BVec;
use rand::{SeedableRng, rngs::SmallRng, Rng};

fn main() {
    let start = std::time::Instant::now();

    let mut rng = SmallRng::from_seed([0; 32]);
    let n = 10_000;

    let mut b = BVec::new();
    for value in 0..n {
        let index = rng.gen_range(0..=b.len());
        b.insert(index, value)
    }
    let t1 = start.elapsed().as_millis();

    let mut x = 0;
    for _ in 0..100_000_000 {
        let index = rng.gen_range(0..b.len());
        x ^= b[index];
    }

    println!("{x}, {t1}, {}", start.elapsed().as_millis());
    std::mem::forget(b);
}
