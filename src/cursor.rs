use core::{
    marker::PhantomData,
    mem::MaybeUninit,
    ptr::{self, NonNull},
};

use crate::{
    node::handle::{InsertResult, Internal, InternalMut, LeafMut},
    node::Node,
    utils::{free_leaf, slice_shift_left, slice_shift_right},
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
// TODO: Variance
pub struct CursorMut<'a, T, const B: usize, const C: usize> {
    leaf_index: usize,
    index: usize,
    // TODO: marker for this?
    // maybe just use `*mut`?
    tree: NonNull<BTreeVec<T, B, C>>,
    path: [MaybeUninit<*mut Option<Node<T, B, C>>>; usize::BITS as usize],
    _marker: PhantomData<&'a mut Node<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> CursorMut<'a, T, B, C> {
    pub(crate) fn new_at(mut tree: NonNull<BTreeVec<T, B, C>>, mut index: usize) -> Self {
        let len = unsafe { tree.as_ref().len() };
        if index > len {
            panic!();
        }

        let mut path: [MaybeUninit<*mut Option<Node<T, B, C>>>; usize::BITS as usize] =
            unsafe { MaybeUninit::uninit().assume_init() };

        if unsafe { tree.as_ref().root.is_none() } {
            return Self {
                path,
                index: 0,
                tree,
                leaf_index: 0,
                _marker: PhantomData,
            };
        }

        let is_past_the_end = index == len;

        if is_past_the_end {
            index -= 1;
        }

        let j = index;
        let height = unsafe { tree.as_ref().height };
        let node = unsafe { &mut tree.as_mut().root };
        // the height of `cur_node` is `height`
        let mut cur_node = node;
        path[height].write(cur_node);
        // decrement the height of `cur_node` `height` times
        'h: for h in (0..height).rev() {
            let handle = unsafe { InternalMut::new(cur_node) };
            for child in handle
                .into_children_slice_mut()
                .iter_mut()
                .take_while(|x| x.is_some())
            {
                if index < child.as_ref().unwrap().len() {
                    cur_node = child;
                    path[h].write(cur_node);
                    continue 'h;
                }
                index -= child.as_ref().unwrap().len();
            }
            unreachable!();
        }

        Self {
            path,
            index: j + usize::from(is_past_the_end),
            tree,
            leaf_index: index + usize::from(is_past_the_end),
            _marker: PhantomData,
        }
    }

    fn height(&self) -> usize {
        unsafe { self.tree.as_ref().height }
    }

    pub(crate) fn len(&self) -> usize {
        unsafe { self.tree.as_ref().len() }
    }

    pub fn insert(&mut self, value: T) {
        if unsafe { self.tree.as_ref().root.is_none() } {
            unsafe {
                self.tree.as_mut().height = 0;
                self.tree.as_mut().root = Some(Node::from_value(value));
                self.path[0].write(&mut self.tree.as_mut().root);
            }
            self.index = 0;
            self.leaf_index = 0;
            return;
        }

        let mut height = 0;
        let mut leaf = unsafe { LeafMut::new(&mut *self.path[0].assume_init()) };
        let mut to_insert = leaf.insert_value(self.leaf_index, value);

        if let InsertResult::SplitRight(_) = to_insert {
            self.leaf_index -= leaf.len();
        }

        while let InsertResult::SplitLeft(_) | InsertResult::SplitRight(_) = to_insert {
            height += 1;
            unsafe {
                if height <= self.height() {
                    let mut parent = InternalMut::new(&mut *self.path[height].assume_init());
                    let child_ptr = self.path[height - 1].assume_init();
                    let child_index = parent.index_of_child_ptr(child_ptr);
                    let (path_through_new, new_node) = match to_insert {
                        InsertResult::SplitRight(n) => (true, n),
                        InsertResult::SplitLeft(n) => (false, n),
                        _ => unreachable!(),
                    };
                    to_insert = parent.insert_node(child_index + 1, new_node);
                    if let InsertResult::SplitMiddle(n) = to_insert {
                        to_insert = if path_through_new {
                            InsertResult::SplitRight(n)
                        } else {
                            InsertResult::SplitLeft(n)
                        };
                    }
                    let path_index = if path_through_new {
                        child_index + 1
                    } else {
                        child_index
                    };
                    let ptr: *mut _ = match to_insert {
                        InsertResult::Fit | InsertResult::SplitLeft(_) => {
                            parent.get_child_mut(path_index)
                        }
                        InsertResult::SplitRight(ref mut n) => {
                            &mut (*n.ptr.children.as_ptr())[path_index - B / 2 - 1]
                        }
                        InsertResult::SplitMiddle(_) => unreachable!(),
                    };
                    self.path[height - 1].write(ptr);
                } else {
                    let node = self.tree.as_mut().root.take().unwrap();
                    let (new_node, update_ptr) = match to_insert {
                        InsertResult::SplitLeft(n) => (n, false),
                        InsertResult::SplitRight(n) => (n, true),
                        _ => unreachable!(),
                    };

                    self.tree.as_mut().height = height;
                    self.tree.as_mut().root = Some(Node::from_child_array([node, new_node]));

                    self.path[height - 1].write(
                        &mut InternalMut::new(&mut self.tree.as_mut().root).children_slice_mut()
                            [usize::from(update_ptr)],
                    );
                    self.path[height].write(&mut self.tree.as_mut().root);
                    return;
                }
            }
        }

        height += 1;
        unsafe {
            while height <= self.height() {
                let node = self.path[height].assume_init().as_mut().unwrap();
                height += 1;
                let len = node.as_ref().unwrap().len();
                node.as_mut().unwrap().set_len(len + 1);
            }
        }
    }

    pub fn remove(&mut self) -> T {
        if unsafe { self.tree.as_ref().root.as_ref().is_none() } {
            panic!();
        }

        let ret;

        // handle root being a leaf
        unsafe {
            if self.height() == 0 {
                debug_assert_eq!(self.leaf_index, self.index);
                let mut leaf = LeafMut::new(&mut self.tree.as_mut().root);
                if self.leaf_index >= leaf.len() {
                    // TODO: better
                    panic!("out of bounds");
                }
                if leaf.len() > 1 {
                    ret = leaf.remove_no_underflow(self.leaf_index);
                } else {
                    ret = leaf.values_mut().as_mut_ptr().read();
                    free_leaf(self.tree.as_mut().root.take().unwrap());
                    self.tree.as_mut().root = None;
                }
                return ret;
            }
        }

        // remove element from leaf
        unsafe {
            // root is internal
            let mut parent = InternalMut::new(&mut *self.path[1].assume_init());
            let child_ptr = self.path[0].assume_init();
            let mut self_index = parent.index_of_child_ptr(child_ptr);

            // Do this before because reasons?
            parent.set_len(parent.len() - 1);
            let mut leaf = LeafMut::new(&mut *self.path[0].assume_init());
            if leaf.is_almost_underfull() {
                if self_index > 0 {
                    ret = combine_leaves_tail(&mut parent, &mut self.leaf_index, &mut self_index);
                } else {
                    ret = combine_leaves_head(&mut parent, self.leaf_index);
                }
                self.path[0].write(parent.get_child_mut(self_index));
            } else {
                ret = leaf.remove_no_underflow(self.leaf_index);
            }
        }

        // update lengths and merge nodes if needed
        unsafe {
            // height of `cur_node`
            for height in 1..self.height() {
                let mut parent = InternalMut::new(&mut *self.path[height + 1].assume_init());
                // TODO: make another loop after non-underfull?
                let cur_ptr = self.path[height].assume_init();
                let cur_node = InternalMut::new(&mut *cur_ptr);

                parent.set_len(parent.len() - 1);

                if cur_node.is_underfull() {
                    let mut parent_index = parent.index_of_child_ptr(cur_ptr);
                    let (mut fst, mut snd, combine_res);

                    if parent_index == 0 {
                        fst = InternalMut::new(&mut *cur_ptr);
                        snd = InternalMut::new(parent.get_child_mut(1));
                        parent_index += 1;
                        combine_res = combine_internals_fst_underfull(fst, snd);
                    } else {
                        let child_index =
                            cur_node.index_of_child_ptr(self.path[height - 1].assume_init());
                        fst = InternalMut::new(parent.get_child_mut(parent_index - 1));
                        snd = InternalMut::new(&mut *cur_ptr);
                        combine_res = combine_internals_snd_underfull(fst, snd);
                        fst = InternalMut::new(parent.get_child_mut(parent_index - 1));
                        snd = InternalMut::new(&mut *cur_ptr);
                        let new_child_ptr: *mut _ = match combine_res {
                            CombineResult::Ok => &mut snd.children_slice_mut()[child_index + 1],
                            CombineResult::Merged => {
                                &mut fst.children_slice_mut()[child_index + (B - 1) / 2 + 1]
                            }
                        };
                        self.path[height - 1].write(new_child_ptr);
                    }

                    if let CombineResult::Merged = combine_res {
                        slice_shift_left(parent.children_slice_range_mut(parent_index..), None);
                        self.path[height].write(parent.get_child_mut(parent_index - 1));
                    }
                }
            }
        }

        // move cursor to start of next leaf if pointing past the end of the current leaf
        unsafe {
            if self.leaf_index
                == self.path[0]
                    .assume_init()
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .len()
            {
                for height in 1..=self.height() {
                    let mut parent = InternalMut::new(&mut *self.path[height].assume_init());
                    let parent_index =
                        parent.index_of_child_ptr(self.path[height - 1].assume_init());
                    if parent_index + 1 < B {
                        if let n @ Some(_) = parent.get_child_mut(parent_index + 1) {
                            let mut cur = n as *mut _;
                            for h in (0..height).rev() {
                                self.path[h].write(cur);
                                if h > 0 {
                                    cur = InternalMut::new(&mut *cur).get_child_mut(0);
                                }
                            }
                            self.leaf_index = 0;
                            break;
                        }
                    }
                }
            }
        }

        // move the root one level lower if needed
        unsafe {
            let root_height = self.height();
            // TODO: deduplicate
            let root = Internal::new(self.tree.as_ref().root.as_ref().unwrap());

            if root.is_singleton() {
                let mut old_root = InternalMut::new(&mut self.tree.as_mut().root);
                let new_root = old_root.children_slice_mut()[0].take().unwrap();
                // `old_root` points to the `root` field of `self` so it must be freed before assigning a new root
                old_root.free();

                self.tree.as_mut().height = root_height - 1;
                self.tree.as_mut().root = Some(new_root);
                self.path[root_height - 1].write(&mut self.tree.as_mut().root);
            }
        }

        ret
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
) -> T {
    let ret;

    let [prev, cur]: &mut [Option<Node<T, B, C>>; 2] = (&mut parent.children_slice_mut()
        [*self_index - 1..=*self_index])
        .try_into()
        .unwrap();

    let prev_len = prev.as_ref().unwrap().len();
    let mut prev = unsafe { LeafMut::new(prev) };
    if prev.is_almost_underfull() {
        let mut src_node = cur.take().unwrap();
        let mut src = unsafe { LeafMut::new_node(&mut src_node) };
        let src_len = src.len();
        let dst_ptr = unsafe { prev.values_maybe_uninit_mut().as_mut_ptr().add(prev_len) };
        let src_ptr = src.values_maybe_uninit_mut().as_ptr();

        ret = unsafe { ptr::read(src_ptr.add(*child_index)).assume_init() };

        unsafe {
            ptr::copy_nonoverlapping(src_ptr, dst_ptr, *child_index);
            ptr::copy_nonoverlapping(
                src_ptr.add(*child_index + 1),
                dst_ptr.add(*child_index),
                src_len - 1 - *child_index,
            );
            free_leaf(src_node);

            prev.set_len(src_len + prev_len - 1);
        }
        slice_shift_left(&mut parent.children_slice_mut()[*self_index..], None);
        *self_index -= 1;
        *child_index += prev_len;
    } else {
        unsafe {
            let mut cur = LeafMut::new(cur);

            let x = prev.pop_back();
            ret = cur.values_maybe_uninit_mut()[*child_index].as_ptr().read();
            let cur_ptr = cur.values_maybe_uninit_mut().as_mut_ptr();
            ptr::copy(cur_ptr, cur_ptr.add(1), *child_index);
            cur.values_maybe_uninit_mut()[0].write(x);
            *child_index += 1;
        }
    }

    ret
}

unsafe fn combine_leaves_head<T, const B: usize, const C: usize>(
    parent: &mut InternalMut<T, B, C>,
    child_index: usize,
) -> T {
    let ret;

    let [cur, next]: &mut [Option<Node<T, B, C>>; 2] = (&mut parent.children_slice_mut()[0..=1])
        .try_into()
        .unwrap();

    if unsafe { LeafMut::new(next).is_almost_underfull() } {
        let mut dst = unsafe { LeafMut::new(cur) };
        let mut src_node = next.take().unwrap();
        let mut src = unsafe { LeafMut::new_node(&mut src_node) };
        let src_len = src.len();
        let dst_len = dst.len();
        let dst_ptr = dst.values_maybe_uninit_mut().as_mut_ptr();
        let src_ptr = src.values_maybe_uninit_mut().as_ptr();

        ret = unsafe { ptr::read(dst_ptr.add(child_index)).assume_init() };

        unsafe {
            ptr::copy(
                dst_ptr.add(child_index + 1),
                dst_ptr.add(child_index),
                dst_len - child_index - 1,
            );
            ptr::copy_nonoverlapping(src_ptr, dst_ptr.add(dst_len - 1), src_len);
            free_leaf(src_node);

            dst.set_len(src_len + dst_len - 1);
        }
        slice_shift_left(&mut parent.children_slice_mut()[1..], None);
    } else {
        unsafe {
            let mut next = LeafMut::new(next);
            let mut cur = LeafMut::new(cur);
            let cur_len = cur.len();

            let x = next.pop_front();
            ret = cur.values_maybe_uninit_mut()[child_index].as_ptr().read();
            let cur_ptr = cur.values_maybe_uninit_mut().as_mut_ptr();
            ptr::copy(
                cur_ptr.add(child_index + 1),
                cur_ptr.add(child_index),
                cur_len - 1 - child_index,
            );
            cur.values_maybe_uninit_mut()[cur_len - 1].write(x);
        }
    }

    ret
}

unsafe fn combine_internals_fst_underfull<T, const B: usize, const C: usize>(
    mut fst: InternalMut<T, B, C>,
    mut snd: InternalMut<T, B, C>,
) -> CombineResult {
    if snd.is_almost_underfull() {
        unsafe {
            take_children(fst, snd, (B - 1) / 2, (B - 1) / 2 + 1);
        }
        CombineResult::Merged
    } else {
        let x = slice_shift_left(snd.children_slice_mut(), None).unwrap();

        snd.set_len(snd.len() - x.len());
        fst.set_len(fst.len() + x.len());

        *fst.get_child_mut((B - 1) / 2) = Some(x);
        CombineResult::Ok
    }
}

unsafe fn combine_internals_snd_underfull<T, const B: usize, const C: usize>(
    mut fst: InternalMut<T, B, C>,
    mut snd: InternalMut<T, B, C>,
) -> CombineResult {
    if fst.is_almost_underfull() {
        unsafe {
            take_children(fst, snd, (B - 1) / 2 + 1, (B - 1) / 2);
        }
        CombineResult::Merged
    } else {
        for i in (0..B).rev() {
            if let Some(x) = fst.get_child_mut(i).take() {
                fst.set_len(fst.len() - x.len());
                snd.set_len(snd.len() + x.len());

                slice_shift_right(snd.children_slice_mut(), Some(x));
                return CombineResult::Ok;
            }
        }
        unreachable!();
    }
}

unsafe fn take_children<T, const B: usize, const C: usize>(
    mut fst: InternalMut<T, B, C>,
    mut snd: InternalMut<T, B, C>,
    fst_len: usize,
    snd_len: usize,
) {
    unsafe {
        fst.children_slice_range_mut(fst_len..fst_len + snd_len)
            .swap_with_slice(snd.children_slice_range_mut(..snd_len));
    }
    fst.set_len(fst.len() + snd.len());
    debug_assert!(snd.children().iter().all(Option::is_none));
    snd.free();
}
