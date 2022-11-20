use super::BRANCH_FACTOR;

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

    pub fn from_array(array: [usize; BRANCH_FACTOR]) -> Self {
        let mut this = Self { inner: array };
        this.init();
        this
    }

    pub fn into_array(mut self) -> [usize; BRANCH_FACTOR] {
        self.fini();
        self.inner
    }

    pub fn child_containing_index(&self, index: &mut usize) -> usize {
        let mut i = 0;
        for shift in 1..=BRANCH_FACTOR.trailing_zeros() {
            let offset = BRANCH_FACTOR >> shift;
            let v = self.inner[i + offset - 1];
            if v <= *index {
                *index -= v;
                i += offset;
            }
        }
        i
    }

    pub unsafe fn prefix_sum(&self, mut index: usize) -> usize {
        debug_assert!(index <= self.inner.len());
        let mut sum = 0;
        while index != 0 {
            sum += unsafe { *self.inner.get_unchecked(index - 1) };
            index &= index - 1;
        }
        sum
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
}
