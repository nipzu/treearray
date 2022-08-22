use core::{
    mem::MaybeUninit,
    ops::{Index, IndexMut},
    ptr,
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

pub struct ArrayVecMut<'a, T, const N: usize> {
    array: &'a mut [MaybeUninit<T>; N],
    len: &'a mut usize,
}

impl<'a, T, const N: usize> ArrayVecMut<'a, T, N> {
    pub unsafe fn new(array: &'a mut [MaybeUninit<T>; N], len: &'a mut usize) -> Self {
        debug_assert!(*len <= N);
        Self { array, len }
    }

    pub fn insert(&mut self, index: usize, value: T) {
        assert!(*self.len < N);
        *self.len += 1;
        let tail = &mut self.array[index..*self.len];
        let tail_ptr = tail.as_mut_ptr().cast::<T>();
        unsafe { ptr::copy(tail_ptr, tail_ptr.add(1), tail.len() - 1) };
        unsafe { tail_ptr.write(value) };
    }

    pub fn remove(&mut self, index: usize) -> T {
        assert_ne!(*self.len, 0);
        *self.len -= 1;
        let tail = &mut self.array[index..=*self.len];
        let tail_ptr = tail.as_mut_ptr().cast::<T>();
        let ret = unsafe { tail_ptr.read() };
        unsafe { ptr::copy(tail_ptr.add(1), tail_ptr, tail.len() - 1) };
        ret
    }

    pub fn push_back(&mut self, value: T) {
        self.insert(*self.len, value);
    }

    pub fn pop_back(&mut self) -> T {
        self.remove(*self.len - 1)
    }

    pub fn split(&mut self, index: usize, other: ArrayVecMut<T, N>) {
        assert!(index <= *self.len);
        let tail_len = *self.len - index;
        let src = unsafe { self.array.as_ptr().add(index) };
        let dst = other.array.as_mut_ptr();
        unsafe { ptr::copy_nonoverlapping(src, dst, tail_len) };
        *self.len = index;
        *other.len = tail_len;
    }

    pub fn append(&mut self, other: ArrayVecMut<T, N>) {
        assert!(*self.len + *other.len <= N);
        let src = other.array.as_ptr();
        let dst = unsafe { self.array.as_mut_ptr().add(*self.len) };
        unsafe { ptr::copy_nonoverlapping(src, dst, *other.len) };
        *self.len += *other.len;
        *other.len = 0;
    }
}

impl<'a, T, const N: usize, I> Index<I> for ArrayVecMut<'a, T, N>
where
    [T]: Index<I>,
{
    type Output = <[T] as Index<I>>::Output;
    fn index(&self, index: I) -> &Self::Output {
        unsafe { slice_assume_init_ref(&self.array[..*self.len]).index(index) }
    }
}

impl<'a, T, const N: usize, I> IndexMut<I> for ArrayVecMut<'a, T, N>
where
    [T]: IndexMut<I>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        unsafe { slice_assume_init_mut(&mut self.array[..*self.len]).index_mut(index) }
    }
}
