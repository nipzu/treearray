use core::{mem::MaybeUninit, ptr};

pub fn slice_shift_left<T>(slice: &mut [T], new_end: T) -> T {
    assert!(!slice.is_empty());
    unsafe {
        let slice_ptr = slice.as_mut_ptr();
        let first = ptr::read(slice_ptr);
        ptr::copy(slice_ptr.add(1), slice_ptr, slice.len() - 1);
        ptr::write(slice_ptr.add(slice.len() - 1), new_end);
        first
    }
}

pub fn slice_shift_right<T>(slice: &mut [T], new_start: T) -> T {
    assert!(!slice.is_empty());
    unsafe {
        let slice_ptr = slice.as_mut_ptr();
        let last = ptr::read(slice_ptr.add(slice.len() - 1));
        ptr::copy(slice_ptr, slice_ptr.add(1), slice.len() - 1);
        ptr::write(slice_ptr, new_start);
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
pub unsafe fn slice_assume_init_mut<T>(slice: &mut [MaybeUninit<T>]) -> &mut [T] {
    // SAFETY: similar to safety notes for `slice_get_ref`, but we have a
    // mutable reference which is also guaranteed to be valid for writes.
    unsafe { &mut *(slice as *mut [MaybeUninit<T>] as *mut [T]) }
}
