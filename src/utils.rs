use core::{marker::PhantomData, ptr};

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

    pub fn len(&self) -> usize {
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

    pub fn split(&mut self, other: Self) {
        let len = self.len();
        assert_eq!(len % 2, 0);
        let half_len = len / 2;
        let src = unsafe { self.array.add(half_len) };
        let dst = other.array;
        unsafe {
            ptr::copy_nonoverlapping(src, dst, half_len);
            *self.len = half_len as u16;
            *other.len = half_len as u16;
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
}
