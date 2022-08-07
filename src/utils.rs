use core::{
    mem::MaybeUninit,
    ops::{Index, IndexMut},
    ptr,
};

pub fn slice_shift_left<T>(slice: &mut [T], new_end: T) -> T {
    assert!(!slice.is_empty());
    unsafe {
        let slice_ptr = slice.as_mut_ptr();
        let first = slice_ptr.read();
        ptr::copy(slice_ptr.add(1), slice_ptr, slice.len() - 1);
        slice_ptr.add(slice.len() - 1).write(new_end);
        first
    }
}

pub fn slice_shift_right<T>(slice: &mut [T], new_start: T) -> T {
    assert!(!slice.is_empty());
    unsafe {
        let slice_ptr = slice.as_mut_ptr();
        let last = slice_ptr.add(slice.len() - 1).read();
        ptr::copy(slice_ptr, slice_ptr.add(1), slice.len() - 1);
        slice_ptr.write(new_start);
        last
    }
}

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
        assert!(index <= *self.len);
        *self.len += 1;
        slice_shift_right(&mut self.array[index..*self.len], MaybeUninit::new(value));
    }

    pub fn remove(&mut self, index: usize) -> T {
        assert_ne!(*self.len, 0);
        *self.len -= 1;
        unsafe {
            slice_shift_left(&mut self.array[index..=*self.len], MaybeUninit::uninit())
                .assume_init()
        }
    }

    pub fn push_front(&mut self, value: T) {
        self.insert(0, value);
    }

    pub fn push_back(&mut self, value: T) {
        self.insert(*self.len, value);
    }

    pub fn pop_front(&mut self) -> T {
        self.remove(0)
    }

    pub fn pop_back(&mut self) -> T {
        self.remove(*self.len - 1)
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
