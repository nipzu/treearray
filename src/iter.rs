//! Iterator `struct`s for `BVec`.

// TODO: impl ExactSized and Fused

use core::iter::FusedIterator;

use crate::{cursor::InboundsCursor, BVec};

#[derive(Clone)]
pub struct Iter<'a, T> {
    cursor: Option<InboundsCursor<'a, T>>,
}

impl<'a, T> Iter<'a, T> {
    #[must_use]
    pub(crate) unsafe fn new(v: &'a BVec<T>) -> Self {
        Self {
            cursor: InboundsCursor::try_new_inbounds(v, 0),
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let cursor = self.cursor.take()?;
        let res = cursor.get();
        self.cursor = cursor.move_next();
        Some(res)
    }

    /*
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
            unsafe { self.cursor.move_inbounds_unchecked(n as isize) };
            self.remaining_count -= n + 1;
            unsafe { Some(self.cursor.get_unchecked()) }
        }
    }*/

    // TODO: advance_by
}
impl<'a, T> FusedIterator for Iter<'a, T> {}
// impl<'a, T> ExactSizeIterator for Iter<'a, T> {}
/*

pub struct Drain<'a, T> {
    cursor: CursorMut<'a, T>,
    remaining_count: usize,
}

impl<'a, T> Drain<'a, T> {
    pub(crate) fn new<R>(t: &'a mut BVec<T>, range: R) -> Self
    where
        R: RangeBounds<usize>,
    {
        let start = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(s) => *s,
            Bound::Excluded(s) => s + 1,
        };
        let end = match range.end_bound() {
            Bound::Unbounded => t.len(),
            Bound::Included(s) => s + 1,
            Bound::Excluded(s) => *s,
        };
        assert!(end <= t.len());
        // TODO: start out of bounds
        Self {
            cursor: t.cursor_at_mut(start),
            remaining_count: end.saturating_sub(start),
        }
    }
}*/
/*impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        (self.remaining_count > 0).then(|| {
            self.remaining_count -= 1;
            self.cursor.remove()
        })
    }
}

impl<'a, T> Drop for Drain<'a, T> {
    fn drop(&mut self) {
        for _ in self {}
    }
}
*/
