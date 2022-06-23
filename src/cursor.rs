use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use crate::{
    node::handle::{InsertResult, Internal, InternalMut, Leaf, LeafMut, SplitResult},
    node::Node,
    utils::slice_shift_left,
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
    path: [MaybeUninit<*mut Node<T, B, C>>; usize::BITS as usize],
    _marker: PhantomData<&'a mut BTreeVec<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> CursorMut<'a, T, B, C> {
    pub(crate) fn new_at(mut tree: NonNull<BTreeVec<T, B, C>>, index: usize) -> Self {
        let len = unsafe { tree.as_ref().len() };
        if index > len {
            panic!();
        }

        let mut path = [MaybeUninit::uninit(); usize::BITS as usize];

        if unsafe { tree.as_ref().is_empty() } {
            return Self {
                path,
                index: 0,
                tree,
                leaf_index: 0,
                _marker: PhantomData,
            };
        }

        let is_past_the_end = index == len;
        let mut remaining_index = index;

        if is_past_the_end {
            remaining_index -= 1;
        }

        let height = unsafe { tree.as_ref().height - 1 };
        // the height of `cur_node` is `height`
        let mut cur_node = unsafe { tree.as_mut().root.assume_init_mut() };
        path[height].write(cur_node);
        // decrement the height of `cur_node` `height` times
        for h in (0..height).rev() {
            let handle = unsafe { InternalMut::new(cur_node) };
            cur_node = unsafe { handle.into_child_containing_index(&mut remaining_index) };
            path[h].write(cur_node);
        }

        let leaf_index = remaining_index + usize::from(is_past_the_end);

        Self {
            path,
            index,
            tree,
            leaf_index,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn get(&self) -> Option<&T> {
        if self.leaf_index < unsafe { self.leaf().len() } {
            unsafe { Some(self.leaf().value_unchecked(self.leaf_index)) }
        } else {
            None
        }
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

    fn tree_mut(&mut self) -> &mut BTreeVec<T, B, C> {
        unsafe { self.tree.as_mut() }
    }

    fn root_mut(&mut self) -> &mut MaybeUninit<Node<T, B, C>> {
        &mut self.tree_mut().root
    }

    unsafe fn leaf_mut(&mut self) -> LeafMut<T, B, C> {
        unsafe { LeafMut::new(&mut *self.path[0].assume_init()) }
    }

    unsafe fn leaf(&self) -> Leaf<T, B, C> {
        unsafe { Leaf::new(&*self.path[0].assume_init()) }
    }

    fn height(&self) -> usize {
        self.tree().height
    }

    pub(crate) fn len(&self) -> usize {
        self.tree().len()
    }

    unsafe fn path_internal_mut<'b>(&mut self, height: usize) -> InternalMut<'b, T, B, C> {
        unsafe { InternalMut::from_ptr(self.path[height].assume_init()) }
    }

    unsafe fn path_internal<'b>(&self, height: usize) -> Internal<'b, T, B, C> {
        unsafe { Internal::new(&*self.path[height].assume_init()) }
    }

    unsafe fn insert_to_empty(&mut self, value: T) {
        self.tree_mut().height = 1;
        let root_ptr: *mut _ = self.root_mut().write(Node::from_value(value));
        self.path[0].write(root_ptr);
        self.index = 0;
        self.leaf_index = 0;
    }

    unsafe fn split_root(&mut self, split_res: SplitResult<T, B, C>) {
        let (new_node, root_path_index) = match split_res {
            SplitResult::Left(n) => (n, 0),
            SplitResult::Right(n) => (n, 1),
        };

        let old_root = unsafe { self.root_mut().assume_init_read() };
        self.root_mut()
            .write(Node::from_child_array([old_root, new_node]));

        let old_height = self.height();
        self.tree_mut().height = old_height + 1;

        let child_ptr: *mut _ = unsafe {
            InternalMut::new(self.root_mut().assume_init_mut()).child_mut(root_path_index)
        };
        let root_ptr: *mut _ = unsafe { self.root_mut().assume_init_mut() };

        self.path[old_height - 1].write(child_ptr);
        self.path[old_height].write(root_ptr);
    }

    pub fn insert(&mut self, value: T) {
        if self.tree().is_empty() {
            unsafe { self.insert_to_empty(value) };
            return;
        }

        let leaf_index = self.leaf_index;
        let mut leaf = unsafe { self.leaf_mut() };
        let mut to_insert = leaf.insert_value(leaf_index, value);

        if let InsertResult::Split(SplitResult::Right(_)) = to_insert {
            self.leaf_index -= leaf.len();
        }

        let mut height = 0;
        while let InsertResult::Split(split_res) = to_insert {
            height += 1;

            // should not be >?
            if height >= self.height() {
                debug_assert_eq!(height, self.height());
                unsafe { self.split_root(split_res) };
                return;
            }

            unsafe {
                let (mut parent, child_index) = self.path_node_and_index_of_child(height);
                let path_through_new = matches!(split_res, SplitResult::Right(_));
                let path_index = child_index + usize::from(path_through_new);

                to_insert = parent.insert_node(child_index + 1, split_res);

                let child_path_ptr: *mut _ = match to_insert {
                    InsertResult::Split(SplitResult::Right(ref mut n)) => {
                        n.ptr.children.as_mut().children
                            [path_index - InternalMut::<T, B, C>::UNDERFULL_LEN - 1]
                            .assume_init_mut()
                    }
                    _ => parent.child_mut(path_index),
                };
                self.path[height - 1].write(child_path_ptr);
            }
        }

        for h in height + 1..self.height() {
            let mut node = unsafe { self.path_internal_mut(h) };
            node.set_len(node.len() + 1);
        }
    }

    pub fn remove(&mut self) -> T {
        // TODO: what about past-the-end cursors?
        if self.tree().is_empty() {
            panic!();
        }

        // handle root being a leaf
        if self.height() == 1 {
            return unsafe { self.remove_from_root_leaf() };
        }

        let ret = unsafe { self.remove_from_leaf() };

        // update lengths and merge nodes if needed
        unsafe {
            // height of `cur_node`
            for height in 1..self.height() - 1 {
                let mut parent = self.path_internal_mut(height + 1);
                // TODO: make another loop after non-underfull?
                let cur_ptr = self.path[height].assume_init();
                let mut cur_node = InternalMut::from_ptr(cur_ptr);

                parent.set_len(parent.len() - 1);

                if cur_node.is_underfull() {
                    let mut parent_index = parent.index_of_child_ptr(cur_ptr);
                    let combine_res;

                    if parent_index == 0 {
                        let next = InternalMut::new(parent.child_mut(1));
                        parent_index += 1;
                        combine_res = combine_internals_fst_underfull(cur_node, next);
                    } else {
                        let child_index =
                            cur_node.index_of_child_ptr(self.path[height - 1].assume_init());
                        let mut prev = InternalMut::new(parent.child_mut(parent_index - 1));
                        combine_res =
                            combine_internals_snd_underfull(prev.reborrow(), cur_node.reborrow());

                        let new_child_ptr: *mut _ = match combine_res {
                            CombineResult::Ok => cur_node.child_mut(child_index + 1),
                            CombineResult::Merged => prev
                                .child_mut(child_index + InternalMut::<T, B, C>::UNDERFULL_LEN + 1),
                        };
                        self.path[height - 1].write(new_child_ptr);
                    }

                    if let CombineResult::Merged = combine_res {
                        slice_shift_left(
                            parent.children_range_mut(parent_index..),
                            MaybeUninit::uninit(),
                        );
                        self.path[height].write(parent.child_mut(parent_index - 1));
                    }
                }
            }
        }

        // move cursor to start of next leaf if pointing past the end of the current leaf
        unsafe {
            if self.leaf_index == self.leaf_mut().len() {
                for height in 1..self.height() {
                    let (mut parent, parent_index) = self.path_node_and_index_of_child(height);
                    if parent_index + 1 < parent.count_children() {
                        let n = parent.child_mut(parent_index + 1);
                        let mut cur = n as *mut _;
                        for h in (0..height).rev() {
                            self.path[h].write(cur);
                            if h > 0 {
                                cur = InternalMut::from_ptr(cur).child_mut(0);
                            }
                        }
                        self.leaf_index = 0;
                        break;
                    }
                }
            }
        }

        // move the root one level lower if needed
        unsafe {
            let root_height = self.height();
            // TODO: deduplicate
            let root = Internal::new(self.tree().root.assume_init_ref());

            if root.is_singleton() {
                let mut old_root = InternalMut::new(self.root_mut().assume_init_mut());
                debug_assert!(old_root.children().children().len() == 1);
                let new_root = old_root.children_mut().pop_back();
                // `old_root` points to the `root` field of `self` so it must be freed before assigning a new root
                old_root.free();

                self.tree_mut().height = root_height - 1;
                self.root_mut().write(new_root);
                let root_ptr: *mut _ = self.root_mut().assume_init_mut();
                self.path[root_height - 2].write(root_ptr);
            }
        }

        ret
    }

    unsafe fn remove_from_leaf(&mut self) -> T {
        // TODO: do we even check for out of bounds???

        unsafe {
            // root is internal
            let mut self_index = self.index_of_path_node(0);
            let mut parent = self.path_internal_mut(1);
            // let (mut parent, mut self_index) = self.path_node_and_index_of_child(1);

            // Do this before because reasons?
            parent.set_len(parent.len() - 1);
            let leaf_index = self.leaf_index;
            let mut leaf = self.leaf_mut();

            let ret = leaf.remove_unchecked(leaf_index);

            if leaf.is_underfull() {
                if self_index > 0 {
                    combine_leaves_tail(&mut parent, &mut self.leaf_index, &mut self_index);
                } else {
                    combine_leaves_head(&mut parent);
                };
                self.path[0].write(parent.child_mut(self_index));
            }

            ret
        }
    }

    unsafe fn remove_from_root_leaf(&mut self) -> T {
        unsafe {
            debug_assert_eq!(self.leaf_index, self.index);
            let index = self.leaf_index;
            let mut leaf = LeafMut::new(self.root_mut().assume_init_mut());

            // TODO: better
            assert!(index < leaf.len(), "out of bounds");

            if leaf.len() > 1 {
                leaf.remove_unchecked(index)
            } else {
                let ret = leaf.values_mut().as_ptr().read();
                leaf.free();
                self.tree_mut().height = 0;
                ret
            }
        }
    }

    unsafe fn index_of_path_node(&self, height: usize) -> usize {
        unsafe {
            let parent = self.path_internal(height + 1);
            let node_ptr = self.path[height].assume_init();
            parent.index_of_child_ptr(node_ptr)
        }
    }

    unsafe fn path_node_and_index_of_child(
        &mut self,
        height: usize,
    ) -> (InternalMut<T, B, C>, usize) {
        unsafe {
            let parent = self.path_internal_mut(height);
            let node_ptr = self.path[height - 1].assume_init();
            let index = parent.index_of_child_ptr(node_ptr);
            (parent, index)
        }
    }

    // pub fn move_next(&mut self) {
    //     todo!()
    // }
}

enum CombineResult {
    Ok,
    Merged,
}

unsafe fn combine_leaves_tail<T, const B: usize, const C: usize>(
    parent: &mut InternalMut<T, B, C>,
    child_index: &mut usize,
    self_index: &mut usize,
) {
    let [prev, cur]: &mut [Node<T, B, C>; 2] = (&mut parent.children_mut().children_mut()
        [*self_index - 1..=*self_index])
        .try_into()
        .unwrap();
    let mut prev = unsafe { LeafMut::new(prev) };
    let mut cur = unsafe { LeafMut::new(cur) };

    if prev.is_almost_underfull() {
        let prev_len = prev.len();
        let mut cur = unsafe { parent.children_mut().remove(*self_index) };
        let cur = unsafe { LeafMut::new(&mut cur) };
        let mut prev = unsafe { LeafMut::new(parent.child_mut(*self_index - 1)) };

        unsafe { prev.append_from(cur) }
        *self_index -= 1;
        *child_index += prev_len;
    } else {
        unsafe { cur.push_front(prev.pop_back()) }
        *child_index += 1;
    }
}

unsafe fn combine_leaves_head<T, const B: usize, const C: usize>(
    parent: &mut InternalMut<T, B, C>,
) {
    let [cur, next]: &mut [Node<T, B, C>; 2] = (&mut parent.children_mut().children_mut()[0..=1])
        .try_into()
        .unwrap();
    let mut next = unsafe { LeafMut::new(next) };
    let mut cur = unsafe { LeafMut::new(cur) };

    if next.is_almost_underfull() {
        let mut next = unsafe { parent.children_mut().remove(1) };
        let next = unsafe { LeafMut::new(&mut next) };
        let mut cur = unsafe { LeafMut::new(parent.child_mut(0)) };
        unsafe { cur.append_from(next) }
    } else {
        unsafe { cur.push_back(next.pop_front()) }
    }
}

unsafe fn combine_internals_fst_underfull<T, const B: usize, const C: usize>(
    mut fst: InternalMut<T, B, C>,
    snd: InternalMut<T, B, C>,
) -> CombineResult {
    if snd.is_almost_underfull() {
        let uf = InternalMut::<T, B, C>::UNDERFULL_LEN;
        unsafe { fst.append_from(snd, uf, uf + 1) }
        CombineResult::Merged
    } else {
        unsafe { fst.rotate_from_next(snd) }
        CombineResult::Ok
    }
}

unsafe fn combine_internals_snd_underfull<T, const B: usize, const C: usize>(
    mut fst: InternalMut<T, B, C>,
    mut snd: InternalMut<T, B, C>,
) -> CombineResult {
    if fst.is_almost_underfull() {
        let uf = InternalMut::<T, B, C>::UNDERFULL_LEN;
        unsafe { fst.append_from(snd, uf + 1, uf) }
        CombineResult::Merged
    } else {
        unsafe { snd.rotate_from_previous(fst) }
        CombineResult::Ok
    }
}
