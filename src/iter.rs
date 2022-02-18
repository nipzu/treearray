//! Iterator `struct`s for `BTreeVec`.

// TODO: impl FusedIterator

use crate::{BTreeVec, CursorMut};

pub struct Iter<'a, T, const B: usize, const C: usize> {
    index: usize,
    v: &'a BTreeVec<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Iter<'a, T, B, C> {
    #[must_use]
    pub(crate) const fn new(v: &'a BTreeVec<T, B, C>) -> Self {
        Self { index: 0, v }
    }
}

impl<'a, T, const B: usize, const C: usize> Iterator for Iter<'a, T, B, C> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.v.get(self.index);
        self.index += 1;
        item
    }
}

pub struct Drain<'a, T, const B: usize, const C: usize> {
    cursor: CursorMut<'a, T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Drain<'a, T, B, C> {
    pub(crate) fn new(t: &'a mut BTreeVec<T, B, C>) -> Self {
        Self {
            cursor: t.cursor_at_mut(0),
        }
    }
}

impl<'a, T, const B: usize, const C: usize> Iterator for Drain<'a, T, B, C> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: breaks stacked borrows
        (self.cursor.len() > 0).then(|| self.cursor.remove())
    }
}

impl<'a, T, const B: usize, const C: usize> Drop for Drain<'a, T, B, C> {
    fn drop(&mut self) {
        for _ in self {}
    }
}
