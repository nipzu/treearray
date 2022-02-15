use crate::node::handle::{InternalMut, LeafMut};
use crate::utils::slice_index_of_ref;
use crate::{node::Node, Root};
use core::ptr::NonNull;
use core::{marker::PhantomData, mem::MaybeUninit};

pub struct Cursor<'a, T, const B: usize, const C: usize> {
    leaf_index: usize,
    index: usize,
    path: [MaybeUninit<Option<&'a Node<T, B, C>>>; usize::BITS as usize],
}

impl<'a, T, const B: usize, const C: usize> Cursor<'a, T, B, C> {
    pub(crate) unsafe fn new(
        path: [MaybeUninit<Option<&'a Node<T, B, C>>>; usize::BITS as usize],
        index: usize,
        leaf_index: usize,
    ) -> Self {
        Self {
            path,
            index,
            leaf_index,
        }
    }

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
}

// TODO: auto traits: Send, Sync, Unpin, UnwindSafe?
// TODO: Variance
pub struct CursorMut<'a, T, const B: usize, const C: usize> {
    leaf_index: usize,
    index: usize,
    // TODO: marker for this?
    root: *mut Option<Root<T, B, C>>,
    // TODO: NonNull?
    path: [MaybeUninit<Option<NonNull<Node<T, B, C>>>>; usize::BITS as usize],
    _marker: PhantomData<&'a mut Node<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> CursorMut<'a, T, B, C> {
    pub(crate) unsafe fn new(
        path: [MaybeUninit<Option<NonNull<Node<T, B, C>>>>; usize::BITS as usize],
        index: usize,
        root: *mut Option<Root<T, B, C>>,
        leaf_index: usize,
    ) -> Self {
        Self {
            path,
            index,
            leaf_index,
            root,
            _marker: PhantomData,
        }
    }

    pub fn insert(&mut self, value: T) {
        if unsafe { self.path[0].assume_init().is_none() } {
            self.path[1].write(None);
            unsafe {
                *self.root = Some(Root {
                    height: 0,
                    node: Node::from_value(value),
                });
                self.path[0].write(NonNull::new(&mut (*self.root).as_mut().unwrap().node));
            }
            self.index = 0;
            self.leaf_index = 0;
            return;
        }

        let mut height = 0;
        let mut to_insert = unsafe {
            LeafMut::new(self.path[0].assume_init().unwrap().as_mut()).insert(self.leaf_index, value)
        };

        // TODO: adjust leaf_index

        while let Some(new_node) = to_insert {
            height += 1;
            unsafe {
                if let Some(mut node) = self.path[height].assume_init() {
                    let mut node = InternalMut::new(height, node.as_mut());
                    let child_index = slice_index_of_ref(
                        node.children(),
                        core::mem::transmute(self.path[height - 1].assume_init().unwrap()),
                    );
                    to_insert = node.insert_node(child_index, new_node);
                } else {
                    let Root { node, .. } = (*self.root).take().unwrap();

                    *self.root = Some(Root {
                        height,
                        node: Node::from_child_array([node, new_node]),
                    });

                    self.path[height].write(NonNull::new(&mut (*self.root).as_mut().unwrap().node));
                    self.path[height + 1].write(None);

                    return;
                }
            }
        }

        height += 1;
        unsafe {
            while let Some(mut node) = self.path[height].assume_init() {
                height += 1;
                let new_len = node.as_mut().len() + 1;
                node.as_mut().set_length(new_len);
            }
        }
    }

    pub fn move_next(&mut self) {
        todo!()
    }
}
