use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use crate::{
    node::handle::{
        height, ExactHeightNode, FreeableNode, InsertResult, InternalMut, Leaf, LeafMut, OwnedNode,
        SplitResult,
    },
    node::{InternalNode, Node},
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
    // TODO: maybe just use `*mut`?
    tree: NonNull<BTreeVec<T, B, C>>,
    leaf: MaybeUninit<*mut Node<T, B, C>>,
    parent: MaybeUninit<NonNull<InternalNode<T, B, C>>>,
    _marker: PhantomData<&'a mut BTreeVec<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> CursorMut<'a, T, B, C> {
    pub(crate) fn new_at(tree: &'a mut BTreeVec<T, B, C>, index: usize) -> Self {
        let len = tree.len();
        if index > len {
            panic!();
        }

        if tree.is_empty() {
            return Self {
                index: 0,
                tree: NonNull::from(tree),
                leaf_index: 0,
                leaf: MaybeUninit::uninit(),
                parent: MaybeUninit::uninit(),
                _marker: PhantomData,
            };
        }

        if tree.height == 1 {
            let mut tree = NonNull::from(tree);
            let leaf = unsafe { MaybeUninit::new(tree.as_mut().root.assume_init_mut() as *mut _) };
            return Self {
                index,
                tree,
                leaf_index: index,
                leaf,
                parent: MaybeUninit::uninit(),
                _marker: PhantomData,
            };
        }

        let is_past_the_end = index == len;
        let mut remaining_index = index;

        if is_past_the_end {
            remaining_index -= 1;
        }

        let height = tree.height - 1;
        let mut tree = NonNull::from(tree);
        // the height of `cur_node` is `height`
        let mut cur_node =
            unsafe { InternalMut::new(tree.as_mut().root.assume_init_mut().ptr.children) };
        // decrement the height of `cur_node` `height - 1` times
        for _ in (1..height).rev() {
            let handle = cur_node;
            let new_cur_node = unsafe { handle.into_child_containing_index(&mut remaining_index) };
            cur_node = unsafe { InternalMut::new(new_cur_node.ptr.children) };
        }

        // debug_assert_eq!(cur_node.len(), cur_node.children().sum_lens());
        let (leaf, parent) =
            unsafe { cur_node.into_child_containing_index_with_parent(&mut remaining_index) };

        let leaf_index = remaining_index + usize::from(is_past_the_end);

        Self {
            index,
            parent: MaybeUninit::new(parent),
            tree,
            leaf_index,
            leaf: MaybeUninit::new(leaf),
            _marker: PhantomData,
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
    //     //                     cur = InternalMut::from_ptr(cur).child_mut(0);
    //     //                 }
    //     //             }
    //     //             self.leaf_index = 0;
    //     //             break;
    //     //         }
    //     //     }
    //     // }
    // }

    fn tree(&self) -> &BTreeVec<T, B, C> {
        unsafe { self.tree.as_ref() }
    }

    fn height_mut(&mut self) -> &mut usize {
        unsafe { &mut (*self.tree.as_ptr()).height }
    }

    fn root_mut(&mut self) -> &mut MaybeUninit<Node<T, B, C>> {
        unsafe { &mut (*self.tree.as_ptr()).root }
    }

    fn leaf_mut(&mut self) -> Option<LeafMut<T, B, C>> {
        (self.height() > 0).then(|| unsafe { LeafMut::new_leaf(self.leaf.assume_init()) })
    }

    fn leaf(&self) -> Option<Leaf<T, B, C>> {
        (self.height() > 0).then(|| unsafe { Leaf::new(&*self.leaf.assume_init()) })
    }

    fn height(&self) -> usize {
        self.tree().height
    }

    pub(crate) fn len(&self) -> usize {
        self.tree().len()
    }

    unsafe fn update_path_lengths<F: Fn(usize) -> usize>(&mut self, f: F) {
        if self.height() > 1 {
            unsafe {
                let mut cur_internal = InternalMut::new(self.parent.assume_init());
                while let Some((mut parent, index)) = cur_internal.into_parent_and_index() {
                    let mut child = &mut parent.as_array_vec()[index];
                    child.length = f(child.length);
                    cur_internal = parent.into_internal();
                }
                let mut root = self.root_mut().assume_init_mut();
                root.length = f(root.length);
            }
        }
    }

    fn insert_to_empty(&mut self, value: T) {
        let root_ptr: *mut _ = self.root_mut().write(Node::from_value(value));
        self.leaf = MaybeUninit::new(root_ptr);
        *self.height_mut() = 1;
        self.index = 0;
        self.leaf_index = 0;
    }

    unsafe fn split_root_internal(&mut self, split_res: SplitResult<T, B, C>) {
        let new_node = match split_res {
            SplitResult::Left(n) | SplitResult::Right(n) => n,
        };

        let old_root = unsafe { self.root_mut().assume_init_read() };
        let new_root_ptr: *mut _ = self
            .root_mut()
            .write(Node::from_child_array([old_root, new_node]));
        *self.height_mut() += 1;

        let mut new_root =
            unsafe { InternalMut::new_parent_of_internal((*new_root_ptr).ptr.children) };
        new_root.with_brand(|mut new_root| {
            new_root.set_child_parent_cache(0);
            new_root.set_child_parent_cache(1);
        });
    }

    unsafe fn split_root_leaf(&mut self, split_res: SplitResult<T, B, C>) {
        let (new_node, root_path_index) = match split_res {
            SplitResult::Left(n) => (n, 0),
            SplitResult::Right(n) => (n, 1),
        };

        let old_root = unsafe { self.root_mut().assume_init_read() };
        let new_root_ptr: *mut _ = self
            .root_mut()
            .write(Node::from_child_array([old_root, new_node]));
        *self.height_mut() += 1;

        let mut new_root = unsafe { InternalMut::new_parent_of_leaf((*new_root_ptr).ptr.children) };
        self.leaf
            .write(new_root.child_mut(root_path_index).node_ptr());
        self.parent.write(new_root.node);
    }

    pub fn insert(&mut self, value: T) {
        let leaf_index = self.leaf_index;
        let mut leaf = if let Some(leaf) = self.leaf_mut() {
            leaf
        } else {
            self.insert_to_empty(value);
            return;
        };

        let mut to_insert = leaf.insert_value(leaf_index, value);
        if let InsertResult::Split(SplitResult::Right(_)) = to_insert {
            self.leaf_index -= leaf.len();
        }

        if self.height() == 1 {
            if let InsertResult::Split(split_res) = to_insert {
                unsafe { self.split_root_leaf(split_res) };
            }
            return;
        }

        // height of `cur_node`
        let mut height = 1;
        let cur_node = unsafe { InternalMut::new_parent_of_leaf(self.parent.assume_init()) };

        let mut parent = cur_node;
        if let InsertResult::Split(split_res) = to_insert {
            unsafe {
                let child_index = parent.index_of_child_ptr(self.leaf.assume_init());
                let mut path_index =
                    child_index + usize::from(matches!(split_res, SplitResult::Right(_)));

                to_insert = parent.insert_node(child_index + 1, split_res);
                let children = if let InsertResult::Split(SplitResult::Right(ref mut n)) = to_insert
                {
                    path_index -= InternalMut::<height::One, T, B, C>::UNDERFULL_LEN + 1;
                    InternalMut::new_parent_of_leaf(n.ptr.children).node
                } else {
                    parent.node
                };
                self.leaf.write(
                    (*children.as_ptr())
                        .children
                        .as_mut_ptr()
                        .cast::<Node<T, B, C>>()
                        .add(path_index),
                );
                self.parent.write(children);
            }
        }

        let mut cur_node = parent.into_internal();

        height += 1;

        while let InsertResult::Split(split_res) = to_insert {
            // should not be >?
            if height >= self.height() {
                // debug_assert_eq!(height, self.height());
                unsafe { self.split_root_internal(split_res) };
                return;
            }

            unsafe {
                let (mut parent, child_index) = cur_node.into_parent_and_index().unwrap();
                to_insert = parent.insert_node(child_index + 1, split_res);
                cur_node = parent.into_internal();
                height += 1;
            }
        }

        unsafe {
            self.update_path_lengths(|len| len + 1);
        }
    }

    /// # Panics
    /// panics if pointing past the end
    pub fn remove(&mut self) -> T {
        let height = self.height();
        let mut leaf_index = self.leaf_index;
        let mut leaf = self
            .leaf_mut()
            .expect("attempting to remove from empty tree");
        assert!(leaf_index < leaf.len(), "out of bounds");
        let ret = leaf.remove_child(leaf_index);

        if height == 1 {
            if leaf.len() == 0 {
                unsafe {
                    OwnedNode::new_leaf(self.root_mut().assume_init_read()).free();
                }
                *self.height_mut() = 0;
            }
            debug_assert_eq!(self.leaf_index, self.index);
            return ret;
        }

        // root is internal

        let leaf_underfull = leaf.is_underfull();
        let leaf_ptr = leaf.node_ptr();

        let mut parent = unsafe { InternalMut::new_parent_of_leaf(self.parent.assume_init()) };

        unsafe {
            let mut self_index = parent.index_of_child_ptr(leaf_ptr);
            // parent.set_len(parent.len() - 1);

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

        let mut child_index = unsafe { parent.index_of_child_ptr(self.leaf.assume_init()) };
        let mut cur_node = parent.into_internal();

        // update lengths and merge nodes if needed
        unsafe {
            // height of `cur_node`
            for h in 1..self.height() - 1 {
                // TODO: make another loop after non-underfull?
                let (mut cur_parent, mut cur_index) = cur_node.into_parent_and_index().unwrap();

                cur_parent.with_brand(|mut parent| {
                    let cur_node = parent.child_mut(cur_index);

                    if cur_node.is_underfull() {
                        if cur_index > 0 {
                            parent.handle_underfull_internal_child_tail(
                                &mut cur_index,
                                &mut child_index,
                            );
                        } else {
                            parent.handle_underfull_internal_child_head();
                        }
                        parent.set_child_parent_cache(cur_index);
                        if h == 1 {
                            self.parent.write(parent.child_mut(cur_index).node);
                            self.leaf.write(
                                InternalMut::new_parent_of_leaf(parent.child_mut(cur_index).node)
                                    .child_mut(child_index)
                                    .node_ptr(),
                            );
                        }
                    }
                });
                cur_node = cur_parent.into_internal();
                child_index = cur_index;
            }
        }

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
            //                         cur = InternalMut::new(cur).raw_child_mut(0);
            //                     }
            //                 }
            //                 self.leaf_index = 0;
            //                 break;
            //             }
            //         }
            *self = Self::new_at(unsafe { self.tree.as_mut() }, self.index);
        }
        // }

        // move the root one level lower if needed
        unsafe {
            let mut old_root =
                InternalMut::new((*self.tree.as_ptr()).root.assume_init_mut().ptr.children);

            if old_root.is_singleton() {
                *self.height_mut() -= 1;
                let new_root = old_root.as_array_vec().pop_back();
                // `old_root` points to the `root` field of `self` so it must be freed before assigning a new root
                OwnedNode::new_internal(self.root_mut().assume_init_read()).free();
                let new_root: *mut _ = self.root_mut().write(new_root);
                if self.height() == 1 {
                    self.leaf.write(new_root);
                }
            }
        }

        ret
    }
}
