use core::{
    mem::size_of,
    ptr,
};

use alloc::boxed::Box;

use crate::node::Node;

pub fn slice_insert_forget_last<T>(slice: &mut [T], index: usize, value: T) {
    assert!(index < slice.len());
    unsafe {
        let index_ptr = slice.as_mut_ptr().add(index);
        ptr::copy(index_ptr, index_ptr.add(1), slice.len() - index - 1);
        ptr::write(index_ptr, value);
    }
}

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

pub fn slice_index_of_ptr<T>(slice: &[T], elem: *const T) -> usize {
    let slice_addr = slice.as_ptr() as usize;
    let elem_addr = elem as usize;
    (elem_addr - slice_addr) / size_of::<T>()
}

pub unsafe fn free_internal<T, const B: usize, const C: usize>(node: Node<T, B, C>) {
    unsafe {
        Box::from_raw(node.ptr.children.as_ptr());
    }
}

pub unsafe fn free_leaf<T, const B: usize, const C: usize>(node: Node<T, B, C>) {
    unsafe {
        Box::from_raw(node.ptr.values.as_ptr());
    }
}
