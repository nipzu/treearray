use crate::node::handle::{Internal, Leaf};
use crate::utils::slice_index_of_ref;
use core::mem::MaybeUninit;

pub struct Cursor<'a, T, const B: usize, const C: usize> {
    // index: usize,
    leaf_index: usize,
    leaf: Option<Leaf<'a, T, B, C>>,
    route: [MaybeUninit<Option<Internal<'a, T, B, C>>>; usize::BITS as usize],
}

impl<'a, T, const B: usize, const C: usize> Cursor<'a, T, B, C> {
    pub fn move_forward(&mut self, mut offset: usize) {
        // TODO: what to do when going out of bounds

        if let Some(leaf) = self.leaf.as_mut() {
            if self.leaf_index + offset < leaf.len() {
                self.leaf_index += offset;
                return;
            }

            let mut prev_node = leaf.node();
            offset -= leaf.len() - self.leaf_index;

            'height: for node in self.route.iter_mut() {
                let node = unsafe { node.assume_init_mut() };
                if let Some(node) = node {
                    // FIXME: this transmute is not justified
                    let mut index = slice_index_of_ref(node.children(), unsafe {
                        core::mem::transmute(prev_node)
                    });
                    index += 1;

                    for child in node.children()[index..].iter() {
                        if let Some(child) = child {
                            if offset < child.len() {
                                todo!();
                            }
                            offset -= child.len();
                        } else {
                            prev_node = node.node();
                            continue 'height;
                        }
                    }
                } else {
                    panic!()
                }
            }
        }
    }
}
