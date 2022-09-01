use core::{
    mem::MaybeUninit,
    ops::{Index, IndexMut},
    ptr, slice,
};

// TODO: use functions from core when https://github.com/rust-lang/rust/issues/63569 stabilises

/// Assuming all the elements are initialized, get a mutable slice to them.
///
/// # Safety
///
/// It is up to the caller to guarantee that the `MaybeUninit<T>` elements
/// really are in an initialized state.
/// Calling this when the content is not yet fully initialized causes undefined behavior.
#[inline]
pub const unsafe fn slice_assume_init_ref<T>(slice: &[MaybeUninit<T>]) -> &[T] {
    // SAFETY: similar to safety notes for `slice_get_ref`, but we have a
    // mutable reference which is also guaranteed to be valid for writes.
    unsafe { &*(slice as *const [MaybeUninit<T>] as *const [T]) }
}

/// Assuming all the elements are initialized, get a mutable slice to them.
///
/// # Safety
///
/// It is up to the caller to guarantee that the `MaybeUninit<T>` elements
/// really are in an initialized state.
/// Calling this when the content is not yet fully initialized causes undefined behavior.
#[inline]
pub unsafe fn slice_assume_init_mut<T>(slice: &mut [MaybeUninit<T>]) -> &mut [T] {
    // SAFETY: similar to safety notes for `slice_get_ref`, but we have a
    // mutable reference which is also guaranteed to be valid for writes.
    unsafe { &mut *(slice as *mut [MaybeUninit<T>] as *mut [T]) }
}

pub struct ArrayVecMut<T, const N: usize> {
    array: *mut T,
    len: *mut usize,
}

impl<T, const N: usize> ArrayVecMut<T, N> {
    pub unsafe fn new(array: *mut [MaybeUninit<T>; N], len: *mut usize) -> Self {
        debug_assert!(unsafe { *len <= N });
        Self {
            array: array.cast(),
            len,
        }
    }

    pub fn insert(&mut self, index: usize, value: T) {
        let len = unsafe { *self.len };
        assert!(len < N);
        assert!(index <= len);
        unsafe {
            let tail_ptr = self.array.add(index);
            ptr::copy(tail_ptr, tail_ptr.add(1), len - index);
            tail_ptr.write(value);
            *self.len += 1;
        }
    }

    pub fn remove(&mut self, index: usize) -> T {
        let len = unsafe { *self.len };
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

    pub fn push_back(&mut self, value: T) {
        self.insert(unsafe { *self.len }, value);
    }

    pub fn pop_back(&mut self) -> T {
        self.remove(unsafe { *self.len - 1 })
    }

    pub fn split(&mut self, index: usize, other: Self) {
        let len = unsafe { *self.len };
        assert!(index <= len);
        let tail_len = len - index;
        let src = unsafe { self.array.add(index) };
        let dst = other.array;
        unsafe { ptr::copy_nonoverlapping(src, dst, tail_len) };
        unsafe {
            *self.len = index;
            *other.len = tail_len;
        }
    }

    pub fn append(&mut self, other: Self) {
        assert!(unsafe { *self.len + *other.len <= N });
        let src = other.array;
        let dst = unsafe { self.array.add(*self.len) };
        unsafe { ptr::copy_nonoverlapping(src, dst, *other.len) };
        unsafe {
            *self.len += *other.len;
            *other.len = 0;
        }
    }
}

impl<T, const N: usize, I> Index<I> for ArrayVecMut<T, N>
where
    [T]: Index<I>,
{
    type Output = <[T] as Index<I>>::Output;
    fn index(&self, index: I) -> &Self::Output {
        unsafe { slice::from_raw_parts(self.array, *self.len).index(index) }
    }
}

impl<T, const N: usize, I> IndexMut<I> for ArrayVecMut<T, N>
where
    [T]: IndexMut<I>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        unsafe { slice::from_raw_parts_mut(self.array, *self.len).index_mut(index) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_vec_insert_front() {
        let mut a: [MaybeUninit<usize>; 100] = [MaybeUninit::uninit(); 100];
        let mut len = 0;
        let mut r = unsafe { ArrayVecMut::new(&mut a, &mut len) };

        for x in 0..50 {
            r.insert(0, x);
        }

        assert_eq!(&(core::array::from_fn(|i| 49 - i) as [usize; 50]), &r[..])
    }
}
