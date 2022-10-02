//! Iterator `struct`s for `BTreeVec`.

// TODO: impl FusedIterator

use crate::{BTreeVec, CursorMut};

pub struct Iter<'a, T> {
    index: usize,
    v: &'a BTreeVec<T>,
}

impl<'a, T> Iter<'a, T> {
    #[must_use]
    pub(crate) const fn new(v: &'a BTreeVec<T>) -> Self {
        Self { index: 0, v }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.v.get(self.index);
        self.index += 1;
        item
    }
}

pub struct Drain<'a, T> {
    cursor: CursorMut<'a, T>,
}

impl<'a, T> Drain<'a, T> {
    pub(crate) fn new(t: &'a mut BTreeVec<T>) -> Self {
        Self {
            cursor: t.cursor_at_mut(0),
        }
    }
}

impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        (self.cursor.len() > 0).then(|| self.cursor.remove())
    }
}

impl<'a, T> Drop for Drain<'a, T> {
    fn drop(&mut self) {
        for _ in self {}
    }
}
