use core::{
    ops::{Index, IndexMut},
    ptr, slice,
};

pub struct ArrayVecMut<T> {
    array: *mut T,
    len: *mut u16,
    cap: u16
}

impl<T> ArrayVecMut<T> {
    pub unsafe fn new(array: *mut T, len: *mut u16, cap: u16) -> Self {
        debug_assert!(unsafe { usize::from(*len) <= usize::from(cap) });
        Self {
            array: array.cast(),
            len,
            cap,
        }
    }

    pub fn len(&self) -> usize {
        unsafe { usize::from(*self.len) }
    }

    pub fn insert(&mut self, index: usize, value: T) {
        let len = unsafe { *self.len }.into();
        assert!(len < usize::from(self.cap));
        assert!(index <= len);
        unsafe {
            let tail_ptr = self.array.add(index);
            ptr::copy(tail_ptr, tail_ptr.add(1), len - index);
            tail_ptr.write(value);
            *self.len += 1;
        }
    }

    pub fn remove(&mut self, index: usize) -> T {
        let len = unsafe { *self.len }.into();
        assert_ne!(len, 0);
        assert!(index < len);
        unsafe {
            let tail_ptr = self.array.add(index);
            let ret = tail_ptr.read();
            ptr::copy(tail_ptr.add(1), tail_ptr, len - index - 1);
            *self.len -= 1;
            ret
        }
    }

    pub fn pop_back(&mut self) -> T {
        self.remove(self.len() - 1)
    }

    pub fn split(&mut self, index: usize, other: Self) {
        let len = self.len();
        assert!(index <= len);
        let mut tail_len = len;
        tail_len -= index;
        let src = unsafe { self.array.add(index) };
        let dst = other.array;
        unsafe { ptr::copy_nonoverlapping(src, dst, tail_len) };
        unsafe {
            *self.len = index as u16;
            *other.len = tail_len as u16;
        }
    }

    pub fn append(&mut self, other: Self) {
        assert!(self.len() + other.len() <= usize::from(self.cap));
        let src = other.array;
        let dst = unsafe { self.array.add((*self.len).into()) };
        unsafe { ptr::copy_nonoverlapping(src, dst, (*other.len).into()) };
        unsafe {
            *self.len += *other.len;
            *other.len = 0;
        }
    }
}

impl<T, I> Index<I> for ArrayVecMut<T>
where
    [T]: Index<I>,
{
    type Output = <[T] as Index<I>>::Output;
    fn index(&self, index: I) -> &Self::Output {
        unsafe { slice::from_raw_parts(self.array, (*self.len).into()).index(index) }
    }
}

impl<T, I> IndexMut<I> for ArrayVecMut<T>
where
    [T]: IndexMut<I>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        unsafe { slice::from_raw_parts_mut(self.array, (*self.len).into()).index_mut(index) }
    }
}
