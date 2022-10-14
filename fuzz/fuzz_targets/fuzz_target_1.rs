#![no_main]

use libfuzzer_sys::fuzz_target;
use libfuzzer_sys::arbitrary::Arbitrary;

use bvec::BVec;

#[derive(Arbitrary, Debug)]
enum Action {
    Get(usize),
    Insert(usize, i32),
    Remove(usize),
}

fuzz_target!(|data: Vec<Action>| {
    let mut v = Vec::new();
    let mut b = BVec::<i32>::new();
    for action in data {
        match action {
            Action::Get(i) => assert_eq!(v.get(i), b.get(i)),
            Action::Insert(i, x) => {
                assert_eq!(v.len(), b.len());
                let i = i % (v.len() + 1);
                v.insert(i, x);
                b.insert(i, x);
            },
            Action::Remove(i) => {
                assert_eq!(v.len(), b.len());
                if !v.is_empty() {
                    let i = i % v.len();
                    assert_eq!(v.remove(i), b.remove(i));
                }
            }
        }
    }
});
