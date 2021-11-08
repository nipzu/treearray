use crate::BTreeVec;

pub struct Iter<'a, T, const B: usize, const C: usize> {
    index: usize,
    v: &'a BTreeVec<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Iter<'a, T, B,C> {
    pub const fn new(v: &'a BTreeVec<T, B, C>) -> Self {
        Self { index: 0, v }
    }
}

impl<'a, T, const B: usize, const C: usize> Iterator for Iter<'a, T, B,C> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.v.len() {
            let item = self.v.get(self.index);
            self.index += 1;
            return item;
        }
        None
    }
}
