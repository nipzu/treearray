use crate::node::BRANCH_FACTOR;

const OFFSETS: [usize; BRANCH_FACTOR.trailing_zeros() as usize] = {
    let mut offsets = [0; BRANCH_FACTOR.trailing_zeros() as usize];
    let mut i = 0;
    while i < BRANCH_FACTOR.trailing_zeros() {
        offsets[i as usize] = BRANCH_FACTOR >> (i + 1);
        i += 1;
    }
    offsets
};

#[derive(Clone)]
pub struct FenwickTree {
    inner: [usize; BRANCH_FACTOR],
}

impl FenwickTree {
    pub fn new() -> Self {
        Self {
            inner: [0; BRANCH_FACTOR],
        }
    }

    pub fn into_array(mut self) -> [usize; BRANCH_FACTOR] {
        self.fini();
        self.inner
    }

    pub fn child_containing_index(&self, mut index: usize) -> (usize, usize) {
        let mut i = 0;
        for offset in &OFFSETS {
            let v = self.inner[i + offset - 1];
            if v <= index {
                index -= v;
                i += offset;
            }
        }
        (index, i)
    }

    pub fn child_containing_index_inclusive(&self, mut index: usize) -> (usize, usize) {
        let mut i = 0;
        for offset in &OFFSETS {
            let v = self.inner[i + offset - 1];
            if v < index {
                index -= v;
                i += offset;
            }
        }
        (index, i)
    }

    pub fn add_wrapping(&mut self, mut index: usize, amount: usize) {
        while let Some(v) = self.inner.get_mut(index) {
            *v = v.wrapping_add(amount);
            index |= index + 1;
        }
    }

    pub fn total_len(&self) -> usize {
        *self.inner.last().unwrap()
    }

    fn init(&mut self) {
        for index in 0..self.inner.len() {
            let j = index | (index + 1);
            if j < self.inner.len() {
                self.inner[j] += self.inner[index];
            }
        }
    }

    fn fini(&mut self) {
        for index in (0..self.inner.len()).rev() {
            let j = index | (index + 1);
            if j < self.inner.len() {
                self.inner[j] -= self.inner[index];
            }
        }
    }

    pub fn with_flat_lens<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [usize; BRANCH_FACTOR]) -> R,
    {
        self.fini();
        let ret = f(&mut self.inner);
        self.init();
        ret
    }

    pub fn split(&mut self) -> Self {
        let mut other = Self {
            inner: [0; BRANCH_FACTOR],
        };

        other.inner[..BRANCH_FACTOR / 2].copy_from_slice(&self.inner[BRANCH_FACTOR / 2..]);
        self.inner[BRANCH_FACTOR / 2..].fill(0);

        self.inner[BRANCH_FACTOR - 1] = self.inner[BRANCH_FACTOR / 2 - 1];
        other.inner[BRANCH_FACTOR / 2 - 1] -= self.inner[BRANCH_FACTOR - 1];
        other.inner[BRANCH_FACTOR - 1] = other.inner[BRANCH_FACTOR / 2 - 1];

        other
    }
}

#[cfg(test)]
mod tests {
    use super::{FenwickTree, BRANCH_FACTOR};

    #[test]
    fn test_fenwick_into_array() {
        let mut a = FenwickTree::new();
        for i in 0..BRANCH_FACTOR {
            a.add_wrapping(i, i);
        }

        let b = a.into_array();
        for i in 0..BRANCH_FACTOR {
            assert_eq!(i, b[i]);
        }
    }

    #[test]
    fn test_fenwick_init() {
        let mut a = FenwickTree::new();
        for i in 0..BRANCH_FACTOR {
            a.inner[i] = i;
        }
        a.init();

        let mut rem = 0;
        let mut c = 0;
        for i in 0..BRANCH_FACTOR * (BRANCH_FACTOR - 1) / 2 {
            rem += 1;
            if rem >= c {
                c += 1;
                rem = 0;
            }
            assert_eq!((rem, c), a.child_containing_index(i), "{i}");
        }
    }

    #[test]
    fn test_fenwick_split() {
        let mut a = FenwickTree::new();
        for i in 0..BRANCH_FACTOR {
            a.add_wrapping(i, i);
        }

        let b = a.split();
        let a_array = a.into_array();
        let b_array = b.into_array();
        let mut a_correct = [0; BRANCH_FACTOR];
        let mut b_correct = [0; BRANCH_FACTOR];
        for i in 0..BRANCH_FACTOR / 2 {
            a_correct[i] = i;
        }
        for i in BRANCH_FACTOR / 2..BRANCH_FACTOR {
            b_correct[i - BRANCH_FACTOR / 2] = i;
        }
        assert_eq!(a_array, a_correct);
        assert_eq!(b_array, b_correct);
    }

    #[test]
    fn test_fenwick_add_and_rank() {
        let mut a = FenwickTree::new();

        for i in 0..BRANCH_FACTOR {
            a.add_wrapping(i, i);
        }

        let mut rem = 0;
        let mut c = 0;
        for i in 0..BRANCH_FACTOR * (BRANCH_FACTOR - 1) / 2 {
            rem += 1;
            if rem >= c {
                c += 1;
                rem = 0;
            }
            assert_eq!((rem, c), a.child_containing_index(i), "{i}");
        }
    }

    #[test]
    fn test_fenwick_add_and_rank_inclusive() {
        let mut a = FenwickTree::new();

        for i in 0..BRANCH_FACTOR {
            a.add_wrapping(i, i);
        }

        let mut rem = 0;
        let mut c = 0;
        for i in 0..BRANCH_FACTOR * (BRANCH_FACTOR - 1) / 2 {
            assert_eq!((rem, c), a.child_containing_index_inclusive(i), "{i}");
            if rem >= c {
                c += 1;
                rem = 0;
            }
            rem += 1;
        }
    }

    #[test]
    fn test_fenwick_init_fini() {
        let mut a = FenwickTree::new();

        for i in 0..BRANCH_FACTOR {
            a.with_flat_lens(|arr| arr[i] = i);
        }

        let mut rem = 0;
        let mut c = 0;
        for i in 0..BRANCH_FACTOR * (BRANCH_FACTOR - 1) / 2 {
            assert_eq!((rem, c), a.child_containing_index_inclusive(i), "{i}");
            if rem >= c {
                c += 1;
                rem = 0;
            }
            rem += 1;
        }
    }
}
