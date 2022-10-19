//! Iterator `struct`s for `BVec`.

// TODO: impl FusedIterator

use core::iter::FusedIterator;

use crate::{cursor::CursorInner, ownership, BVec, CursorMut};

#[derive(Clone)]
pub struct Iter<'a, T> {
    cursor: CursorInner<'a, ownership::Immut<'a>, T>,
    remaining_count: usize,
}

impl<'a, T> Iter<'a, T> {
    #[must_use]
    pub(crate) unsafe fn new(v: &'a BVec<T>, start: usize, end: usize) -> Self {
        Self {
            cursor: CursorInner::new(v, start),
            remaining_count: end - start,
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        (self.remaining_count > 0).then(|| {
            let ret = unsafe { self.cursor.get_unchecked() };
            self.remaining_count -= 1;
            if self.remaining_count != 0 {
                self.cursor.move_next_inbounds_unchecked();
            }
            ret
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining_count, Some(self.remaining_count))
    }

    fn count(self) -> usize {
        self.remaining_count
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.remaining_count {
            self.remaining_count = 0;
            None
        } else {
            self.cursor.move_(n as isize);
            self.remaining_count -= n;
            self.remaining_count -= 1;
            unsafe { Some(self.cursor.get_unchecked()) }
        }
    }

    // TODO: advance_by
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {}
impl<'a, T> FusedIterator for Iter<'a, T> {}

pub struct Drain<'a, T> {
    cursor: CursorMut<'a, T>,
}

impl<'a, T> Drain<'a, T> {
    pub(crate) fn new(t: &'a mut BVec<T>) -> Self {
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
