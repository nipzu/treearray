use core::{
    marker::PhantomData,
    ops::{Index, IndexMut},
    ptr, slice,
};

pub struct ArrayVecMut<'a, T> {
    array: *mut T,
    len: *mut u16,
    cap: u16,
    _p: PhantomData<&'a mut T>,
}

impl<'a, T> ArrayVecMut<'a, T> {
    pub unsafe fn new(array: *mut T, len: *mut u16, cap: u16) -> Self {
        debug_assert!(unsafe { *len <= cap });
        Self {
            array: array.cast(),
            len,
            cap,
            _p: PhantomData,
        }
    }

    fn len(&self) -> usize {
        unsafe { usize::from(*self.len) }
    }

    pub fn insert(&mut self, index: usize, value: T) {
        let len = self.len();
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
        let len = self.len();
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

    pub fn split(&mut self, index: usize, other: Self) {
        let len = self.len();
        assert!(index <= len);
        let mut tail_len = len;
        tail_len -= index;
        let src = unsafe { self.array.add(index) };
        let dst = other.array;
        unsafe {
            ptr::copy_nonoverlapping(src, dst, tail_len);
            *self.len = index as u16;
            *other.len = tail_len as u16;
        }
    }

    pub fn append(&mut self, other: Self) {
        assert!(self.len() + other.len() <= usize::from(self.cap));
        let src = other.array;
        let dst = unsafe { self.array.add((*self.len).into()) };
        unsafe {
            ptr::copy_nonoverlapping(src, dst, (*other.len).into());
            *self.len += *other.len;
            *other.len = 0;
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        (index < self.len()).then(|| unsafe { &mut *self.array.add(index) })
    }
}

impl<'a, T, I> Index<I> for ArrayVecMut<'a, T>
where
    [T]: Index<I>,
{
    type Output = <[T] as Index<I>>::Output;
    fn index(&self, index: I) -> &Self::Output {
        unsafe { slice::from_raw_parts(self.array, (*self.len).into()).index(index) }
    }
}

impl<'a, T, I> IndexMut<I> for ArrayVecMut<'a, T>
where
    [T]: IndexMut<I>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        unsafe { slice::from_raw_parts_mut(self.array, (*self.len).into()).index_mut(index) }
    }
}
