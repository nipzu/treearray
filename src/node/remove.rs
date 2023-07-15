use core::{mem, ptr};

use super::handle::LeafMut;
use super::BRANCH_FACTOR;
use super::{InternalNode, NodeBase, RawNodeWithLen};

impl<'a, T: 'a> LeafMut<'a, T> {
    const UNDERFULL_LEN: usize = (NodeBase::<T>::LEAF_CAP - 1) / 2;
    pub fn remove_child(&mut self, index: usize) -> T {
        self.values_mut().remove(index)
    }
    fn push_front_child(&mut self, child: T) {
        self.values_mut().insert(0, child);
    }
    fn push_back_child(&mut self, child: T) {
        let len = self.len();
        self.values_mut().insert(len, child);
    }
    fn pop_front_child(&mut self) -> T {
        self.remove_child(0)
    }
    fn pop_back_child(&mut self) -> T {
        self.remove_child(self.len() - 1)
    }
    pub fn is_underfull(&self) -> bool {
        self.len() <= Self::UNDERFULL_LEN
    }
    fn is_almost_underfull(&self) -> bool {
        self.len() <= Self::UNDERFULL_LEN + 1
    }
}

impl<T> InternalNode<T> {
    unsafe fn append_children(&mut self, other: &mut InternalNode<T>) {
        unsafe { self.append_lengths(other) };
        self.children().append(other.children());
    }

    unsafe fn steal_length_from_next(&mut self, index: usize, amount: usize) {
        unsafe {
            self.add_length_wrapping(index + 1, amount.wrapping_neg());
            self.add_length_wrapping(index, amount);
        }
    }

    unsafe fn steal_length_from_previous(&mut self, index: usize, amount: usize) {
        unsafe {
            self.add_length_wrapping(index - 1, amount.wrapping_neg());
            self.add_length_wrapping(index, amount);
        }
    }

    unsafe fn append_lengths(&mut self, other: &mut InternalNode<T>) {
        let self_len_children = self.len_children();
        let other_len_children = other.len_children();
        let other_lens = other.lengths.clone().into_array();
        self.lengths.with_flat_lens(|lens| {
            lens[self_len_children..self_len_children + other_len_children]
                .copy_from_slice(&other_lens[..other_len_children]);
        });
    }

    // TODO: should be in fenwick
    unsafe fn merge_length_from_next(&mut self, index: usize) {
        self.lengths.with_flat_lens(|lens| unsafe {
            let lens_ptr = lens.as_mut_ptr();
            let next_len = lens_ptr.add(index + 1).read();
            (*lens_ptr.add(index)) += next_len;
            ptr::copy(
                lens_ptr.add(index + 2),
                lens_ptr.add(index + 1),
                BRANCH_FACTOR - index - 2,
            );
            lens[BRANCH_FACTOR - 1] = 0;
        });
    }

    fn is_almost_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN + 1
    }

    pub fn handle_underfull_leaf_child_head(&mut self) {
        let [mut cur, mut next] = unsafe { self.child_pair_at(0) };
        let [mut cur, mut next] = unsafe { [cur.leaf_mut(), next.leaf_mut()] };

        unsafe {
            if next.is_almost_underfull() {
                cur.values_mut().append(next.values_mut());
                self.merge_length_from_next(0);
                drop(self.children().remove(1).into_leaf());
            } else {
                cur.push_back_child(next.pop_front_child());
                self.steal_length_from_next(0, 1);
            }
        }
    }

    pub fn handle_underfull_leaf_child_tail(&mut self, index: usize) {
        let [mut prev, mut cur] = unsafe { self.child_pair_at(index - 1) };
        let [mut prev, mut cur] = unsafe { [prev.leaf_mut(), cur.leaf_mut()] };

        unsafe {
            if prev.is_almost_underfull() {
                prev.values_mut().append(cur.values_mut());
                self.merge_length_from_next(index - 1);
                drop(self.children().remove(index).into_leaf());
            } else {
                cur.push_front_child(prev.pop_back_child());
                self.steal_length_from_previous(index, 1);
            }
        }
    }

    pub fn maybe_handle_underfull_child(&mut self, index: usize) -> bool {
        let is_child_underfull = unsafe {
            self.children[index]
                .assume_init_mut()
                .internal_mut()
                .is_underfull()
        };

        if is_child_underfull {
            if index > 0 {
                self.handle_underfull_internal_child_tail(index);
            } else {
                self.handle_underfull_internal_child_head();
            }
        }

        is_child_underfull
    }

    fn handle_underfull_internal_child_head(&mut self) {
        let [mut cur, mut next] = unsafe { self.child_pair_at(0) };
        let [cur, next] = unsafe { [cur.internal_mut(), next.internal_mut()] };

        if next.is_almost_underfull() {
            unsafe {
                cur.append_children(next);
                self.merge_length_from_next(0);
                drop(self.children().remove(1).into_internal());
            }
        } else {
            unsafe {
                let x = next.pop_front_child();
                let x_len = x.0;
                cur.push_back_child(x);
                self.steal_length_from_next(0, x_len);
            }
        }
    }

    fn handle_underfull_internal_child_tail(&mut self, index: usize) {
        let [mut prev, mut cur] = unsafe { self.child_pair_at(index - 1) };
        let [prev, cur] = unsafe { [prev.internal_mut(), cur.internal_mut()] };

        if prev.is_almost_underfull() {
            unsafe {
                prev.append_children(cur);
                self.merge_length_from_next(index - 1);
                drop(self.children().remove(index).into_internal());
            }
        } else {
            unsafe {
                let x = prev.pop_back_child();
                let x_len = x.0;
                cur.push_front_child(x);
                self.steal_length_from_previous(index, x_len);
            }
        }
    }

    unsafe fn push_front_child(&mut self, child: RawNodeWithLen<T>) {
        unsafe {
            self.push_front_length(child.0);
            self.children().insert(0, child.1);
        }
    }

    pub unsafe fn push_back_child(&mut self, child: RawNodeWithLen<T>) {
        unsafe { self.push_back_length(child.0) };
        let len_children = self.len_children();
        self.children().insert(len_children, child.1);
    }

    unsafe fn pop_front_child(&mut self) -> RawNodeWithLen<T> {
        let node_len = unsafe { self.pop_front_length() };
        let node = self.children().remove(0);
        RawNodeWithLen(node_len, node)
    }

    unsafe fn pop_back_child(&mut self) -> RawNodeWithLen<T> {
        let last_len = unsafe { self.pop_back_length() };
        let len_children = self.len_children();
        let last = self.children().remove(len_children - 1);
        RawNodeWithLen(last_len, last)
    }

    unsafe fn pop_front_length(&mut self) -> usize {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            let first_len = lens[0];
            for i in 1..len_children {
                lens[i - 1] = lens[i];
            }
            *lens.last_mut().unwrap() = 0;
            first_len
        })
    }

    unsafe fn pop_back_length(&mut self) -> usize {
        let len_children = self.len_children();
        self.lengths
            .with_flat_lens(|lens| mem::take(&mut lens[len_children - 1]))
    }

    unsafe fn push_back_length(&mut self, len: usize) {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            lens[len_children] = len;
        });
    }

    unsafe fn push_front_length(&mut self, len: usize) {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            for i in (0..len_children).rev() {
                lens[i + 1] = lens[i];
            }
            lens[0] = len;
        });
    }
}
