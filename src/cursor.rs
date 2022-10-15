use core::mem::MaybeUninit;

use crate::{
    node::{
        handle::{Internal, InternalMut, Leaf, LeafMut, LeafRef, Node, SplitResult},
        InternalNode, NodeBase, NodePtr, RawNodeWithLen,
    },
    BVec,
};

// TODO: auto traits: Send, Sync, Unpin, UnwindSafe?
pub struct CursorMut<'a, T> {
    leaf_index: usize,
    index: usize,
    tree: &'a mut BVec<T>,
    leaf: MaybeUninit<NodePtr<T>>,
}

impl<'a, T> CursorMut<'a, T> {
    pub(crate) fn new(tree: &'a mut BVec<T>, index: usize) -> Self {
        if index > tree.len() {
            panic!();
        }

        if tree.is_empty() {
            return Self {
                index: 0,
                tree,
                leaf_index: 0,
                leaf: MaybeUninit::uninit(),
            };
        }

        let is_past_the_end = index == tree.len();
        let mut cursor = Self::new_inbounds(tree, index - usize::from(is_past_the_end));
        if is_past_the_end {
            cursor.leaf_index += 1;
            cursor.index += 1;
        }
        cursor
    }

    pub(crate) fn new_inbounds(tree: &'a mut BVec<T>, index: usize) -> Self {
        if index >= tree.len() {
            panic!();
        }

        let mut cur_node = tree.root.unwrap();
        let mut target_index = index;

        // the height of `cur_node` is `tree.height - 1`
        // decrement the height of `cur_node` `tree.height - 1` times
        while unsafe { cur_node.as_ref().height() > 0 } {
            let handle = unsafe { InternalMut::new(cur_node) };
            cur_node = unsafe { handle.into_child_containing_index(&mut target_index) };
        }

        Self {
            index,
            tree,
            leaf_index: target_index,
            leaf: MaybeUninit::new(cur_node),
        }
    }

    #[must_use]
    pub fn get(&self) -> Option<&T> {
        self.leaf().and_then(|leaf| leaf.value(self.leaf_index))
    }

    pub fn move_(&mut self, offset: isize) {
        // if (offset as usize) > self.len() - self.index {
        //     panic!();
        // }

        // TODO: overflow
        if self.index.wrapping_add(offset as usize) > self.len() {
            panic!();
        }

        if self.len() == 0 {
            return;
        }

        let mut offset = offset as usize;
        let leaf_len = self.leaf_mut().unwrap().len();

        // fast path
        // TODO: why no over/underflow problems?
        if self.leaf_index.wrapping_add(offset) < leaf_len {
            self.leaf_index = self.leaf_index.wrapping_add(offset);
            return;
        }

        let mut new_parent = self.leaf_mut().unwrap().into_parent_and_index2();
        offset = offset.wrapping_add(self.leaf_index);
        while let Some((mut parent, index)) = new_parent {
            offset = offset.wrapping_add(parent.sum_lens_below(index));
            if offset < parent.len() {
                let mut cur_node = parent.node_ptr();
                while unsafe { cur_node.as_ref().height() > 0 } {
                    let handle = unsafe { InternalMut::new(cur_node) };
                    cur_node = unsafe { handle.into_child_containing_index(&mut offset) };
                }
                self.leaf.write(cur_node);
                self.leaf_index = offset;
                return;
            }
            new_parent = parent.into_parent_and_index2();
        }

        panic!("possibly out of bounds");
    }

    fn root_mut(&mut self) -> &mut Option<NodePtr<T>> {
        &mut self.tree.root
    }

    // TODO: this should not be unbounded?
    fn leaf_mut<'b>(&mut self) -> Option<LeafMut<'b, T>>
    where
        T: 'b,
    {
        (!self.tree.is_empty()).then(|| unsafe { LeafMut::new(self.leaf.assume_init()) })
    }

    fn leaf(&self) -> Option<LeafRef<T>> {
        (!self.tree.is_empty()).then(|| unsafe { LeafRef::new(self.leaf.assume_init()) })
    }

    pub(crate) fn len(&self) -> usize {
        self.tree.len()
    }

    unsafe fn add_path_lengths_wrapping(&mut self, amount: usize) {
        unsafe {
            let mut new_parent = self.leaf_mut().and_then(Node::into_parent_and_index2);

            while let Some((mut parent, index)) = new_parent {
                parent.add_length_wrapping(index, amount);
                new_parent = parent.into_parent_and_index2();
            }
        }
    }

    unsafe fn insert_to_empty(&mut self, value: T) {
        let new_root = NodeBase::new_leaf();
        unsafe { LeafMut::new(new_root).values_mut().insert(0, value) };
        *self.root_mut() = Some(new_root);
        self.leaf.write(new_root);
    }

    unsafe fn split_root(&mut self, new_node: RawNodeWithLen<T>) {
        let old_root = self.root_mut().unwrap();
        let old_root_len = self.tree.len();
        *self.root_mut() = Some(InternalNode::from_child_array([
            RawNodeWithLen(old_root_len, old_root),
            new_node,
        ]));
    }

    pub fn insert(&mut self, value: T) {
        unsafe { self.add_path_lengths_wrapping(1) };

        let leaf_index = self.leaf_index;
        let Some(mut leaf) = self.leaf_mut() else {
            unsafe { self.insert_to_empty(value) };
            return;
        };

        let mut to_insert = leaf.insert_value(leaf_index, value).map(|res| match res {
            SplitResult::Left(n) => n,
            SplitResult::Right(n) => {
                self.leaf_index -= leaf.len();
                self.leaf.write(n.1);
                n
            }
        });

        let mut new_parent = leaf.into_parent_and_index2();

        while let Some(new_node) = to_insert {
            unsafe {
                if let Some((mut parent, child_index)) = new_parent {
                    to_insert = parent.insert_split_of_child(child_index, new_node);
                    new_parent = parent.into_parent_and_index2();
                } else {
                    self.split_root(new_node);
                    return;
                }
            }
        }
    }

    /// # Panics
    /// panics if pointing past the end
    pub fn remove(&mut self) -> T {
        if self.index >= self.tree.len() {
            panic!("index out of bounds");
        }

        unsafe { self.add_path_lengths_wrapping(1_usize.wrapping_neg()) };

        let mut leaf_index = self.leaf_index;
        let mut leaf = self
            .leaf_mut()
            .expect("attempting to remove from empty tree");
        assert!(leaf_index < leaf.len(), "out of bounds");
        let ret = leaf.remove_child(leaf_index);

        let leaf_underfull = leaf.is_underfull();
        let leaf_is_empty = leaf.len() == 0;

        let Some((mut parent, mut self_index)) = (unsafe { leaf.into_parent_and_index3() }) else {
            // height is 1
            if leaf_is_empty {
                unsafe { Leaf::new(self.root_mut().take().unwrap()).free() };
            }
            debug_assert_eq!(self.leaf_index, self.index);
            return ret;
        };

        if leaf_underfull {
            if self_index > 0 {
                parent.handle_underfull_leaf_child_tail(&mut self_index, &mut leaf_index);
                self.leaf_index = leaf_index;
                self.leaf.write(parent.child_mut(self_index).node_ptr());
            } else {
                parent.handle_underfull_leaf_child_head();
            }

            let mut parent = unsafe { parent.into_parent_and_index() };

            // merge nodes if needed
            unsafe {
                while let Some((mut parent_node, cur_index)) = parent {
                    if !parent_node.maybe_handle_underfull_child(cur_index) {
                        break;
                    }
                    parent = parent_node.into_parent_and_index();
                }
            }
        }

        // TODO: maybe don't do this here
        // move cursor to start of next leaf if pointing past the end of the current leaf
        // unsafe {
        if self.leaf().unwrap().len() == leaf_index {
            //         for height in 1..self.height() {
            //             let (mut parent, parent_index) = self.path_node_and_index_of_child(height);
            //             if parent_index + 1 < parent.count_children() {
            //                 let mut cur: *mut _ = parent.raw_child_mut(parent_index + 1);
            //                 for h in (0..height).rev() {
            //                     self.path_mut()[h] = cur;
            //                     if h > 0 {
            //                         cur = NodeMut::new(cur).raw_child_mut(0);
            //                     }
            //                 }
            //                 self.leaf_index = 0;
            //                 break;
            //             }
            //         }
            unsafe {
                *self = Self::new(&mut *(self.tree as *mut _), self.index);
            }
        }
        // }

        // move the root one level lower if needed
        unsafe {
            let mut old_root = InternalMut::new(self.root_mut().unwrap());

            if old_root.is_singleton() {
                let mut new_root = old_root.children().pop_back();
                new_root.as_mut().parent = None;
                // `old_root` points to the `root` field of `self` so it must be freed before assigning a new root
                Internal::new(self.root_mut().unwrap()).free();
                *self.root_mut() = Some(new_root);
            }
        }

        ret
    }
}
