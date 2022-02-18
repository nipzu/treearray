use crate::node::handle::{InternalMut, LeafMut};
use crate::node::VariantMut;
use crate::utils::{slice_index_of_ref, slice_shift_left, slice_shift_right};
use crate::{node::Node, Root};
use core::mem::{self, ManuallyDrop};
use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};
use core::{marker::PhantomData, mem::MaybeUninit};

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
    root: NonNull<Option<Root<T, B, C>>>,
    // TODO: should this be null terminated or just use height from root
    path: [MaybeUninit<*mut Node<T, B, C>>; usize::BITS as usize],
    _marker: PhantomData<&'a mut Node<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> CursorMut<'a, T, B, C> {
    pub(crate) fn new_at(mut root: NonNull<Option<Root<T, B, C>>>, mut index: usize) -> Self {
        let len = unsafe { root.as_ref().as_ref().map_or(0, |n| n.node.len()) };
        if index > len {
            panic!();
        }

        let mut path: [MaybeUninit<*mut Node<T, B, C>>; usize::BITS as usize] =
            unsafe { MaybeUninit::uninit().assume_init() };

        if unsafe { root.as_ref().is_none() } {
            return Self {
                path,
                index: 0,
                root,
                leaf_index: 0,
                _marker: PhantomData,
            };
        }

        let is_past_the_end = index == len;

        if is_past_the_end {
            index -= 1;
        }

        let mut i = unsafe { root.as_ref().as_ref().unwrap().height };
        let mut cur_node = unsafe { root.as_mut().as_mut().unwrap().as_dyn_mut() };

        debug_assert_eq!(cur_node.height(), i);
        path[i].write(cur_node.node_ptr_mut());

        let j = index;

        'd: while let VariantMut::Internal { handle } = cur_node.into_variant_mut() {
            for child in handle.into_children_mut() {
                if index < child.len() {
                    cur_node = child;
                    i -= 1;
                    debug_assert_eq!(cur_node.height(), i);
                    path[i].write(cur_node.node_ptr_mut());
                    continue 'd;
                }
                index -= child.len();
            }
            unreachable!();
        }

        Self {
            path,
            index: j + usize::from(is_past_the_end),
            root,
            leaf_index: index + usize::from(is_past_the_end),
            _marker: PhantomData,
        }
    }

    fn height(&self) -> usize {
        unsafe { self.root.as_ref().as_ref().unwrap().height }
    }

    pub fn insert(&mut self, value: T) {
        if unsafe { self.root.as_ref().is_none() } {
            unsafe {
                *self.root.as_mut() = Some(Root {
                    height: 0,
                    node: Node::from_value(value),
                });
                self.path[0].write(&mut self.root.as_mut().as_mut().unwrap().node);
            }
            self.index = 0;
            self.leaf_index = 0;
            return;
        }

        let mut height = 0;
        let mut to_insert = unsafe {
            LeafMut::new(&mut *self.path[0].assume_init()).insert(self.leaf_index, value)
        };

        // TODO: adjust leaf_index

        while let Some(new_node) = to_insert {
            height += 1;
            unsafe {
                if height <= self.height() {
                    let mut node = InternalMut::new(height, &mut *self.path[height].assume_init());
                    let child_index = slice_index_of_ref(
                        node.children(),
                        &*self.path[height - 1].assume_init().cast(),
                    );
                    to_insert = node.insert_node(child_index, new_node);
                } else {
                    let Root { node, .. } = self.root.as_mut().take().unwrap();

                    *self.root.as_mut() = Some(Root {
                        height,
                        node: Node::from_child_array([node, new_node]),
                    });

                    self.path[height].write(&mut self.root.as_mut().as_mut().unwrap().node);

                    return;
                }
            }
        }

        height += 1;
        unsafe {
            while height <= self.height() {
                let node = self.path[height].assume_init();
                height += 1;
                let new_len = (*node).len() + 1;
                (*node).set_length(new_len);
            }
        }
    }

    pub fn remove(&mut self) -> T {
        if unsafe { self.root.as_ref().is_none() } {
            panic!();
        }

        let ret;
        unsafe {
            if self.root.as_ref().as_ref().unwrap().height == 0 {
                let mut leaf = LeafMut::new(&mut *self.path[0].assume_init());
                if leaf.len() > 1 {
                    ret = leaf.remove_no_underflow(self.leaf_index);
                    leaf.set_len(leaf.len() - 1);
                } else {
                    ret = leaf.values_mut().as_mut_ptr().read();
                    leaf.free();
                    *self.root.as_mut() = None;
                }
                return ret;
            }
        }

        let mut vacancy = None;

        // root is internal
        // assume:
        {
            let mut parent = unsafe { InternalMut::new(1, &mut *self.path[1].assume_init()) };
            let child_index = self.leaf_index;
            let self_index = slice_index_of_ref(parent.children(), unsafe {
                &*self.path[0].assume_init().cast()
            });
            let mut handle = unsafe { LeafMut::new(&mut *self.path[0].assume_init()) };

            if handle.len() - 1 > C / 2 {
                unsafe {
                    ret = handle.remove_no_underflow(child_index);
                    handle.set_len(handle.len() - 1);
                    parent.set_len(parent.len() - 1);
                }
            } else {
                if self_index > 0 {
                    let [prev, cur]: &mut [Option<Node<T, B, C>>; 2] = (&mut parent
                        .children_slice_mut()[self_index - 1..=self_index])
                        .try_into()
                        .unwrap();

                    if prev.as_ref().unwrap().len() == C / 2 + 1 {
                        let dst = prev.as_mut().unwrap();
                        let mut src = cur.take().unwrap();
                        let dst_ptr = unsafe { (*dst.ptr.values).as_mut_ptr().add(C / 2 + 1) };
                        let src_ptr = unsafe { src.ptr.values.as_ptr() };

                        ret = unsafe { ptr::read(src_ptr.add(child_index)).assume_init() };

                        unsafe {
                            ptr::copy_nonoverlapping(src_ptr, dst_ptr, child_index);
                            ptr::copy_nonoverlapping(
                                src_ptr.add(child_index + 1),
                                dst_ptr.add(child_index),
                                C / 2 - child_index,
                            );
                            ManuallyDrop::drop(&mut src.ptr.values);
                            mem::forget(src);
                        }

                        dst.length = NonZeroUsize::new(C).unwrap();
                        vacancy = Some(self_index);
                    } else {
                        unsafe {
                            let mut prev = LeafMut::new(prev.as_mut().unwrap());
                            let cur = cur.as_mut().unwrap();

                            let x = prev.pop_back();
                            ret = cur.ptr.values[child_index].as_ptr().read();
                            let cur_ptr = (*cur.ptr.values).as_mut_ptr();
                            ptr::copy(cur_ptr, cur_ptr.add(1), child_index);
                            (*cur.ptr.values)[0].write(x);
                        }
                    }
                } else {
                    let [cur, next]: &mut [Option<Node<T, B, C>>; 2] = (&mut parent
                        .children_slice_mut()[0..=1])
                        .try_into()
                        .unwrap();

                    if next.as_ref().unwrap().len() == C / 2 + 1 {
                        let dst = cur.as_mut().unwrap();
                        let mut src = next.take().unwrap();
                        let dst_ptr = unsafe { (*dst.ptr.values).as_mut_ptr() };
                        let src_ptr = unsafe { src.ptr.values.as_ptr() };

                        ret = unsafe { ptr::read(dst_ptr.add(child_index)).assume_init() };

                        unsafe {
                            ptr::copy(
                                dst_ptr.add(child_index + 1),
                                dst_ptr.add(child_index),
                                dst.len() - child_index - 1,
                            );
                            ptr::copy_nonoverlapping(
                                src_ptr,
                                dst_ptr.add(dst.len() - 1),
                                src.len(),
                            );
                            ManuallyDrop::drop(&mut src.ptr.values);
                        }
                        mem::forget(src);

                        dst.length = NonZeroUsize::new(C).unwrap();
                        vacancy = Some(1);
                    } else {
                        unsafe {
                            let mut next = LeafMut::new(next.as_mut().unwrap());
                            let cur = cur.as_mut().unwrap();

                            let x = next.pop_front();
                            ret = cur.ptr.values[child_index].as_ptr().read();
                            let cur_ptr = (*cur.ptr.values).as_mut_ptr();
                            ptr::copy(
                                cur_ptr.add(child_index + 1),
                                cur_ptr.add(child_index),
                                C / 2 - child_index,
                            );
                            (*cur.ptr.values)[C / 2].write(x);
                        }
                    }
                };

                unsafe {
                    // FIXME: when B < 5
                    parent.set_len(parent.len() - 1);
                }
                // debug_assert_eq!(self.len(), sum_lens(self.children()));
            }
        }
        {
            let mut height = 2;
            while height <= self.height() {
                unsafe {
                    let mut cur_node =
                        InternalMut::new(height, &mut *self.path[height].assume_init());
                    let mut self_index = slice_index_of_ref(
                        cur_node.children(),
                        &*self.path[height - 1].assume_init().cast(),
                    );
                    let mut child = InternalMut::new(
                        height - 1,
                        cur_node.children_slice_mut()[self_index].as_mut().unwrap(),
                    );

                    match vacancy {
                        None => {
                            // debug_assert_eq!(self.len(), sum_lens(self.children()));
                        }
                        Some(vacant_index) => {
                            // debug_assert!(handle.children()[vacant_index].is_none());
                            slice_shift_left(&mut child.children_slice_mut()[vacant_index..], None);

                            vacancy = None;

                            if child.children()[B / 2].is_none() {
                                if self_index == 0 {
                                    self_index += 1;
                                }

                                let [ref mut fst, ref mut snd]: &mut [Option<Node<T, B, C>>; 2] =
                                    (&mut cur_node.children_slice_mut()
                                        [self_index - 1..=self_index])
                                        .try_into()
                                        .unwrap();

                                if let CombineResult::Merged =
                                    combine_internals(fst, snd, height - 1)
                                {
                                    vacancy = Some(self_index);
                                    // debug_assert!(self.children()[self_index].is_none());
                                    // debug_assert!(self.children()[self_index - 1].is_some());
                                }
                            }
                        }
                    }

                    cur_node.set_len(cur_node.len() - 1);
                }
                height += 1;
                // debug_assert_eq!(self.len(), sum_lens(self.children()));
            }
        }

        if let Some(vacant_index) = vacancy {
            unsafe {
                let mut root = InternalMut::new(
                    self.height(),
                    &mut self.root.as_mut().as_mut().unwrap().node,
                );
                slice_shift_left(&mut root.children_slice_mut()[vacant_index..], None);
            }
        }

        unsafe {
            let root_height = self.height();
            let mut root = InternalMut::new(
                self.height(),
                &mut self.root.as_mut().as_mut().unwrap().node,
            );

            if root.children_mut().count() == 1 {
                let old_root = self.root.as_mut().take().unwrap();

                *self.root.as_mut() = Some(Root {
                    height: root_height - 1,
                    node: old_root.node.ptr.children.as_ptr().read().unwrap(),
                });
            }
        }

        ret
    }

    pub fn move_next(&mut self) {
        todo!()
    }
}

enum CombineResult {
    Ok,
    Merged,
}

unsafe fn combine_internals<T, const B: usize, const C: usize>(
    opt_fst: &mut Option<Node<T, B, C>>,
    opt_snd: &mut Option<Node<T, B, C>>,
    height: usize,
) -> CombineResult {
    let mut fst = unsafe { InternalMut::new(height, opt_fst.as_mut().unwrap()) };
    let mut snd = unsafe { InternalMut::new(height, opt_snd.as_mut().unwrap()) };
    let fst_underfull = fst.children()[B / 2].is_none();
    let snd_underfull = snd.children()[B / 2].is_none();
    let fst_almost_underfull = fst.children()[B / 2 + 1].is_none();
    let snd_almost_underfull = snd.children()[B / 2 + 1].is_none();

    if fst_underfull && snd_almost_underfull {
        fst.children_slice_mut()[B / 2..].swap_with_slice(&mut snd.children_slice_mut()[..=B / 2]);
        unsafe {
            fst.set_len(fst.len() + snd.len());
        }
        *opt_snd = None;
        // debug_assert_eq!(fst.len(), sum_lens(fst.children()));
        return CombineResult::Merged;
    }

    if fst_almost_underfull && snd_underfull {
        fst.children_slice_mut()[B / 2 + 1..]
            .swap_with_slice(&mut snd.children_slice_mut()[..B / 2]);
        unsafe {
            fst.set_len(fst.len() + snd.len());
        }
        *opt_snd = None;
        // debug_assert_eq!(fst.len(), sum_lens(fst.children()));
        return CombineResult::Merged;
    }

    if fst_underfull {
        let x = slice_shift_left(snd.children_slice_mut(), None).unwrap();
        unsafe {
            snd.set_len(snd.len() - x.len());
            fst.set_len(fst.len() + x.len());
        }
        fst.children_slice_mut()[B / 2] = Some(x);
        // debug_assert_eq!(fst.len(), sum_lens(fst.children()));
        // debug_assert_eq!(snd.len(), sum_lens(snd.children()));
        return CombineResult::Ok;
    }

    if snd_underfull {
        let mut i = B - 1;
        loop {
            if fst.children_slice_mut()[i].is_some() {
                let x = fst.children_slice_mut()[i].take().unwrap();

                unsafe {
                    fst.set_len(fst.len() - x.len());
                    snd.set_len(snd.len() + x.len());
                }

                slice_shift_right(snd.children_slice_mut(), Some(x));
                // debug_assert_eq!(fst.len(), sum_lens(fst.children()));
                // debug_assert_eq!(snd.len(), sum_lens(snd.children()));
                return CombineResult::Ok;
            }
            i -= 1;
        }
    }

    CombineResult::Ok
}
