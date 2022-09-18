use core::{mem::MaybeUninit, ptr::NonNull};

use crate::{
    node::{
        handle::{ExactHeightNode, FreeableNode, Leaf, LeafMut, NodeMut, OwnedNode, SplitResult},
        InternalNode, LeafNode, NodeBase, NodePtr,
    },
    BTreeVec,
};

// pub struct Cursor<'a, T, const B: usize, const C: usize> {
//     leaf_index: usize,
//     index: usize,
//     path: [MaybeUninit<Option<&'a Node<T, B, C>>>; usize::BITS as usize],
// }

// impl<'a, T, const B: usize, const C: usize> Cursor<'a, T, B, C> {
//     pub(crate) unsafe fn new(
//         path: [MaybeUninit<Option<&'a Node<T, B, C>>>; usize::BITS as usize],
//         index: usize,
//         leaf_index: usize,
//     ) -> Self {
//         Self {
//             path,
//             index,
//             leaf_index,
//         }
//     }

//     pub fn move_forward(&mut self, mut offset: usize) {
//         // TODO: what to do when going out of bounds

//         if let Some(leaf) = self.leaf.as_mut() {
//             if self.leaf_index + offset < leaf.len() {
//                 self.leaf_index += offset;
//                 return;
//             }

//             let mut prev_node = leaf.node();
//             offset -= leaf.len() - self.leaf_index;

//             'height: for node in self.path.iter_mut() {
//                 let node = unsafe { node.assume_init_mut() };
//                 if let Some(node) = node {
//                     // FIXME: this transmute is not justified
//                     let mut index = slice_index_of_ref(node.children(), unsafe {
//                         core::mem::transmute(prev_node)
//                     });
//                     index += 1;

//                     for child in node.children()[index..].iter() {
//                         if let Some(child) = child {
//                             if offset < child.len() {
//                                 todo!();
//                             }
//                             offset -= child.len();
//                         } else {
//                             prev_node = node.node();
//                             continue 'height;
//                         }
//                     }
//                 } else {
//                     panic!()
//                 }
//             }
//         }
//     }
// }

// TODO: auto traits: Send, Sync, Unpin, UnwindSafe?
pub struct CursorMut<'a, T, const B: usize, const C: usize> {
    leaf_index: usize,
    index: usize,
    tree: &'a mut BTreeVec<T, B, C>,
    leaf: MaybeUninit<NonNull<NodeBase<T, B, C>>>,
}

impl<'a, T, const B: usize, const C: usize> CursorMut<'a, T, B, C> {
    pub(crate) fn new(tree: &'a mut BTreeVec<T, B, C>, index: usize) -> Self {
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

    pub(crate) fn new_inbounds(tree: &'a mut BTreeVec<T, B, C>, index: usize) -> Self {
        if index >= tree.len() {
            panic!();
        }

        let mut cur_node = unsafe { tree.root.assume_init() };
        let mut target_index = index;

        // the height of `cur_node` is `tree.height - 1`
        // decrement the height of `cur_node` `tree.height - 1` times
        for _ in 1..tree.height {
            let handle = unsafe { NodeMut::new_internal(cur_node) };
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

    // pub fn move_right(&mut self, offset: usize) {
    //     if self.len() == 0 {
    //         panic!();
    //     }

    //     let leaf_len = unsafe { self.leaf_mut().len() };

    //     // fast path
    //     // equivalent to self.leaf_index + offset < leaf_len,
    //     // but avoids overflow with large offsets
    //     if offset <= leaf_len - self.leaf_index {
    //         self.leaf_index += offset;
    //         return;
    //     }

    //     let mut height = self.height();
    //     let mut remaining_len = leaf_len - self.leaf_index;
    //     while remaining_len < offset {
    //         height += 1;
    //         // remaining_len += self.path_internal_mut(height).
    //     }

    //     // for height in 1..=self.height() {
    //     //     let mut parent = self.path_internal_mut(height);
    //     //     let parent_index =
    //     //         parent.index_of_child_ptr(self.path[height - 1].assume_init());
    //     //     if parent_index + 1 < B {
    //     //         if let n @ Some(_) = parent.child_mut(parent_index + 1) {
    //     //             let mut cur = n as *mut _;
    //     //             for h in (0..height).rev() {
    //     //                 self.path[h].write(cur);
    //     //                 if h > 0 {
    //     //                     cur = NodeMut::from_ptr(cur).child_mut(0);
    //     //                 }
    //     //             }
    //     //             self.leaf_index = 0;
    //     //             break;
    //     //         }
    //     //     }
    //     // }
    // }

    fn height_mut(&mut self) -> &mut usize {
        &mut self.tree.height
    }

    fn root_mut(&mut self) -> &mut MaybeUninit<NonNull<NodeBase<T, B, C>>> {
        &mut self.tree.root
    }

    // TODO: this should not be unbpunded?
    fn leaf_mut<'b>(&mut self) -> Option<LeafMut<'b, T, B, C>> {
        (self.height() > 0).then(|| unsafe { LeafMut::new_leaf(self.leaf.assume_init()) })
    }

    fn leaf(&self) -> Option<Leaf<T, B, C>> {
        (self.height() > 0).then(|| unsafe { Leaf::new(self.leaf.assume_init().cast().as_ref()) })
    }

    fn height(&self) -> usize {
        self.tree.height
    }

    pub(crate) fn len(&self) -> usize {
        self.tree.len()
    }

    unsafe fn update_path_lengths<F: Fn(usize) -> usize>(&mut self, f: F) {
        if self.height() > 0 {
            unsafe {
                let mut cur_node = self.leaf.assume_init();
                while let Some(parent) = (*cur_node.as_ptr()).parent {
                    let index = (*cur_node.as_ptr()).parent_index.assume_init();
                    let mut parent = NodeMut::new_internal(parent);
                    let mut length: &mut usize =
                        (*parent.internal_ptr()).lengths[index as usize].assume_init_mut();
                    *length = f(*length);
                    cur_node = parent.node_ptr();
                }
                let mut len = &mut self.tree.len;
                *len = f(*len);
            }
        }
    }

    fn insert_to_empty(&mut self, value: T) {
        let root_ptr = self
            .root_mut()
            .write(LeafNode::<T, B, C>::from_value(value).cast());
        self.tree.len = 1;
        self.leaf.write(unsafe { *root_ptr });
        *self.height_mut() = 1;
    }

    unsafe fn split_root(&mut self, new_node_len: usize, new_node: NodePtr<T, B, C>) {
        let mut old_root = unsafe { self.root_mut().assume_init_read() };
        let mut old_root_len = self.tree.len;
        old_root_len -= new_node_len;
        self.root_mut().write(
            InternalNode::from_child_array([(old_root_len, old_root), (new_node_len, new_node)])
                .cast(),
        );
        *self.height_mut() += 1;
    }

    pub fn insert(&mut self, value: T) {
        unsafe { self.update_path_lengths(|len| len + 1) };

        let leaf_index = self.leaf_index;
        let Some(mut leaf) = self.leaf_mut() else {
            self.insert_to_empty(value);
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

        let mut cur_node = leaf.forget_height();

        while let Some((node_len, node)) = to_insert {
            unsafe {
                if let Some((mut parent, child_index)) = cur_node.into_parent_and_index2() {
                    *(*parent.internal_ptr()).lengths[child_index].assume_init_mut() -= node_len;
                    to_insert = parent.insert_node(child_index + 1, (node_len, node));
                    cur_node = parent.forget_height();
                } else {
                    self.split_root(node_len, node);
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

        unsafe {
            self.update_path_lengths(|len| len - 1);
        }

        let mut leaf_index = self.leaf_index;
        let mut leaf = self
            .leaf_mut()
            .expect("attempting to remove from empty tree");
        assert!(leaf_index < leaf.len(), "out of bounds");
        let ret = leaf.remove_child(leaf_index);

        let leaf_underfull = leaf.is_underfull();

        let mut parent = if let Some(parent) = unsafe { (*leaf.node_ptr().as_ptr()).parent } {
            unsafe { NodeMut::new_parent_of_leaf(parent) }
        } else {
            // height is 1
            if leaf.len() == 0 {
                unsafe {
                    OwnedNode::new_leaf(self.root_mut().assume_init_read()).free();
                }
                *self.height_mut() = 0;
            }
            debug_assert_eq!(self.leaf_index, self.index);
            return ret;
        };

        unsafe {
            let mut self_index = (*leaf.node_ptr().as_ptr()).parent_index.assume_init() as usize;

            if leaf_underfull {
                if self_index > 0 {
                    parent.handle_underfull_leaf_child_tail(&mut self_index, &mut leaf_index);
                } else {
                    parent.handle_underfull_leaf_child_head();
                }
                self.leaf.write(parent.child_mut(self_index).node_ptr());
                self.leaf_index = leaf_index;
            }
        }

        let mut cur_node = parent.into_internal();

        // merge nodes if needed
        unsafe {
            while let Some((mut cur_parent, cur_index)) = cur_node.into_parent_and_index() {
                if !cur_parent.maybe_handle_underfull_child(cur_index) {
                    break;
                }
                cur_node = cur_parent.into_internal();
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
            let mut old_root = NodeMut::new_internal(self.tree.root.assume_init());

            if old_root.is_singleton() {
                *self.height_mut() -= 1;
                let mut new_root = old_root.as_array_vec().pop_back();
                new_root.as_mut().parent = None;
                // `old_root` points to the `root` field of `self` so it must be freed before assigning a new root
                OwnedNode::new_internal(self.root_mut().assume_init_read()).free();
                let new_root = self.root_mut().write(new_root);
                if self.height() == 1 {
                    self.leaf.write(*new_root);
                }
            }
        }

        ret
    }
}
