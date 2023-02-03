use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use crate::{
    node::{
        handle::{LeafMut, LeafRef},
        LeafBase, NodeBase, NodePtr,
    },
    ownership, BVec,
};

pub struct CurosrMut2<'a, T> {
    tree: &'a mut BVec<T>,
    leaf: Option<LeafMut<'a, T>>,
    leaf_index: usize,
}

impl<'a, T> CurosrMut2<'a, T> {
    fn new_inbounds(tree: &mut BVec<T>, mut index: usize) -> CurosrMut2<T> {
        if index >= tree.len() {
            panic!();
        }

        let mut cur_node = unsafe { tree.root.assume_init_mut() };
        let height = tree.height;

        // the height of `cur_node` is `height`
        // decrement the height of `cur_node` `height` times
        for _ in 0..height {
            let internal = unsafe { cur_node.internal_mut() };
            let (new_index, child_index) = internal.lengths.child_containing_index(index);
            index = new_index;
            cur_node = unsafe { internal.children[child_index].assume_init_mut() };
        }

        CurosrMut2 {
            leaf: unsafe { Some(LeafMut::new(cur_node.leaf)) },
            tree,
            leaf_index: index,
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        let index = self.leaf_index;
        // TODO: HACK: fix this
        let leaf = unsafe { core::ptr::read(self.leaf.as_mut()?) };
        unsafe { Some(leaf.into_value_unchecked_mut(index)) }
    }
}

#[derive(Clone)]
pub struct Cursor<'a, T> {
    tree: &'a BVec<T>,
    leaf: Option<LeafRef<'a, T>>,
    leaf_index: usize,
}

impl<'a, T> Cursor<'a, T> {
    pub(crate) fn new_ghost(tree: &BVec<T>) -> Cursor<T> {
        Cursor {
            tree,
            leaf: None,
            leaf_index: 0,
        }
    }

    pub(crate) fn try_new_inbounds(tree: &BVec<T>, mut index: usize) -> Option<Cursor<T>> {
        if index >= tree.len() {
            return None;
        }

        let mut cur_node = unsafe { tree.root.assume_init_ref() };
        let height = tree.height;

        // the height of `cur_node` is `height`
        // decrement the height of `cur_node` `height` times
        for _ in 0..height {
            let internal = unsafe { cur_node.internal_ref() };
            let (new_index, child_index) = internal.lengths.child_containing_index(index);
            index = new_index;
            cur_node = unsafe { internal.children[child_index].assume_init_ref() };
        }

        Some(Cursor {
            leaf: unsafe { Some(LeafRef::new(cur_node.leaf)) },
            tree,
            leaf_index: index,
        })
    }

    pub fn move_right(&mut self) {
        if let Some(leaf) = &self.leaf {
            self.leaf_index += 1;
            if leaf.len() == self.leaf_index {
                self.leaf = unsafe { leaf.node.as_ref().next.map(|n| LeafRef::new(n)) };
            }
        } else {
            todo!()
        }
    }

    #[must_use]
    pub fn get(&self) -> Option<&'a T> {
        let leaf = self.leaf.as_ref()?;
        unsafe { Some(leaf.value_unchecked(self.leaf_index)) }
    }
}

/*
// TODO: auto traits: Send, Sync, Unpin, UnwindSafe?
pub struct CursorInner<'a, O, T: 'a>
where
    O: ownership::Reference<'a, T>,
{
    tree: NonNull<BVec<T>>,
    leaf: MaybeUninit<NodePtr<T>>,
    pub leaf_index: usize,
    _marker: PhantomData<(&'a (), O)>,
}

impl<'a, T> Clone for CursorInner<'a, ownership::Immut<'a>, T> {
    fn clone(&self) -> Self {
        Self {
            leaf: self.leaf,
            leaf_index: self.leaf_index,
            tree: self.tree,
            _marker: PhantomData,
        }
    }
}

pub struct Cursor<'a, T> {
    inner: CursorInner<'a, ownership::Immut<'a>, T>,
    index: usize,
}

impl<'a, T> Cursor<'a, T> {
    pub(crate) fn new(tree: &'a BVec<T>, index: usize) -> Self {
        let inner = CursorInner::new(tree, index);
        Self { inner, index }
    }

    #[must_use]
    pub fn get(&self) -> Option<&'a T> {
        self.is_inbounds()
            .then(|| unsafe { self.inner.get_unchecked() })
    }

    #[must_use]
    #[inline]
    pub fn is_inbounds(&self) -> bool {
        self.index < self.len()
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    #[inline]
    pub fn index(&self) -> usize {
        self.index
    }

    /*pub fn move_(&mut self, offset: isize) {
        self.index = self.index.wrapping_add(offset as usize);

        if self.index < self.len() {
            unsafe {
                self.inner.move_inbounds_unchecked(offset);
            }
        } else if self.index == self.len() {
            self.inner = unsafe { CursorInner::new_past_the_end(self.inner.tree.as_ref()) };
        } else {
            panic!()
        }
    }*/
}

pub struct CursorMut<'a, T> {
    inner: CursorInner<'a, ownership::Mut<'a>, T>,
    index: usize,
    _invariant: PhantomData<&'a mut T>,
}

impl<'a, T> CursorMut<'a, T> {
    pub(crate) fn new(tree: &'a mut BVec<T>, index: usize) -> Self {
        let inner = CursorInner::new(tree, index);
        Self {
            inner,
            index,
            _invariant: PhantomData,
        }
    }

    // pub fn as_inbounds(&mut self) -> Option<InboundsCursorMut<T>> {
    //     self.is_inbounds().then(||
    //         InboundsCursorMut { inner: self.inner }
    //     )
    // }

    #[must_use]
    pub fn get(&self) -> Option<&T> {
        self.is_inbounds()
            .then(|| unsafe { self.inner.get_unchecked() })
    }

    #[must_use]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.is_inbounds()
            .then(|| unsafe { self.inner.get_unchecked_mut() })
    }

    #[must_use]
    #[inline]
    pub fn is_inbounds(&self) -> bool {
        self.index < self.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /*pub fn move_(&mut self, offset: isize) {
        self.index = self.index.wrapping_add(offset as usize);
        if self.index < self.len() {
            unsafe {
                self.inner.move_inbounds_unchecked(offset);
            }
        } else if self.index == self.len() {
            self.inner = unsafe { CursorInner::new_past_the_end(self.inner.tree.as_mut()) };
        } else {
            panic!()
        }
    }*/
}

impl<'a, O, T> CursorInner<'a, O, T>
where
    O: ownership::Reference<'a, T>,
{
    pub(crate) fn new(tree: O::RefTy<'a, BVec<T>>, index: usize) -> Self {
        if let Some(c) = Self::try_new_inbounds(unsafe { core::ptr::read(&tree) }, index) {
            c
        } else if index == O::as_ref(&tree).len() {
            Self::new_past_the_end(tree)
        } else {
            panic!();
        }
    }

    pub(crate) fn try_new_inbounds(tree: O::RefTy<'a, BVec<T>>, mut index: usize) -> Option<Self> {
        if index >= O::as_ref(&tree).len() {
            return None;
        }

        let mut cur_node = unsafe { O::as_ref(&tree).root.assume_init() };
        let height = O::as_ref(&tree).height;

        // the height of `cur_node` is `height`
        // decrement the height of `cur_node` `height` times
        for _ in 0..height {
            let handle = unsafe { InternalMut::new(cur_node) };
            cur_node = unsafe { handle.into_child_containing_index(&mut index) };
        }

        Some(Self {
            tree: tree.into(),
            leaf_index: index,
            leaf: MaybeUninit::new(cur_node),
            _marker: PhantomData,
        })
    }

    pub(crate) unsafe fn new_last_unchecked(tree: O::RefTy<'a, BVec<T>>) -> Self {
        debug_assert!(O::as_ref(&tree).is_not_empty());

        let mut cur_node = unsafe { O::as_ref(&tree).root().unwrap_unchecked() };
        let height = O::as_ref(&tree).height;
        for _ in 0..height {
            let mut handle = unsafe { InternalRef::new(cur_node) };
            let len_children = handle.len_children();
            cur_node = unsafe {
                (*handle.internal_ptr())
                    .children
                    .get_unchecked(len_children - 1)
                    .assume_init()
            };
        }

        let leaf_index = unsafe { LeafRef::<T>::new(cur_node).len() - 1 };
        Self {
            tree: tree.into(),
            leaf_index,
            leaf: MaybeUninit::new(cur_node),
            _marker: PhantomData,
        }
    }

    pub(crate) fn new_past_the_end(tree: O::RefTy<'a, BVec<T>>) -> Self {
        if O::as_ref(&tree).is_empty() {
            return Self {
                tree: tree.into(),
                leaf_index: 0,
                leaf: MaybeUninit::uninit(),
                _marker: PhantomData,
            };
        };
        let mut this = unsafe { Self::new_last_unchecked(tree) };
        this.leaf_index += 1;
        this
    }

    pub(crate) fn try_new_inbounds_first(tree: O::RefTy<'a, BVec<T>>) -> Option<Self> {
        let mut cur_node = O::as_ref(&tree).root()?;
        let height = O::as_ref(&tree).height;
        for _ in 0..height {
            let mut handle = unsafe { InternalRef::new(cur_node) };
            cur_node = unsafe { (*handle.internal_ptr()).children[0].assume_init() };
        }

        Some(Self {
            tree: tree.into(),
            leaf: MaybeUninit::new(cur_node),
            leaf_index: 0,
            _marker: PhantomData,
        })
    }

    /*
    pub fn move_(&mut self, offset: isize) {
        // if (offset as usize) > self.len() - self.index {
        //     panic!();
        // }

        // self.len() in 0..=isize::MAX
        // self.len() + offset in (-isize::MIN = isize::MAX + 1) ..= (2*isize::MAX = usize::MAX - 1)

        // TODO: overflow
        // if self.index.wrapping_add(offset as usize) > self.len() {
        //     panic!();
        // }

        if self.tree().is_empty() {
            if offset == 0 {
                return;
            }
            panic!();
        }

        let mut offset = self.leaf_index.wrapping_add(offset as usize);
        let leaf_len = unsafe { self.leaf().unwrap_unchecked().len() };

        // fast path
        // TODO: why no over/underflow problems?
        if offset < leaf_len {
            self.leaf_index = offset;
            return;
        }

        let mut new_parent = unsafe { self.leaf().unwrap_unchecked().into_parent_and_index2() };
        while let Some((mut parent, index)) = new_parent {
            offset = unsafe { offset.wrapping_add(parent.sum_lens_below(index)) };
            if offset < parent.len() {
                let mut cur_node = parent.node_ptr();
                let height = unsafe { cur_node.as_ref().height() };
                for _ in 0..height {
                    let handle = unsafe { InternalMut::new(cur_node) };
                    cur_node = unsafe { handle.into_child_containing_index(&mut offset) };
                }
                self.leaf.write(cur_node);
                self.leaf_index = offset;
                return;
            }
            new_parent = parent.into_parent_and_index2();
        }

        if offset == self.len() {
            let mut cur_node = self.tree().root().unwrap();
            offset -= 1;
            while unsafe { cur_node.as_ref().height() > 0 } {
                let handle = unsafe { InternalMut::new(cur_node) };
                cur_node = unsafe { handle.into_child_containing_index(&mut offset) };
            }
            self.leaf.write(cur_node);
            self.leaf_index = offset + 1;
        } else {
            panic!("out of bounds");
        }
    }

    pub unsafe fn move_inbounds_unchecked(&mut self, offset: isize) {
        let mut offset = self.leaf_index.wrapping_add(offset as usize);
        let leaf_len = unsafe { LeafRef::new(self.leaf.assume_init()).len() };

        // fast path
        // TODO: why no over/underflow problems?
        if offset < leaf_len {
            self.leaf_index = offset;
            return;
        }

        let mut index = unsafe {
            self.leaf
                .assume_init()
                .as_ref()
                .parent_index
                .assume_init()
                .into()
        };
        let mut parent = unsafe {
            InternalRef::<T>::new(self.leaf.assume_init().as_ref().parent.unwrap_unchecked())
        };
        loop {
            offset = unsafe { offset.wrapping_add(parent.sum_lens_below(index)) };
            if offset < parent.len() {
                let mut cur_node = parent.node_ptr();
                let height = unsafe { cur_node.as_ref().height() };
                for _ in 0..height {
                    let handle = unsafe { InternalMut::new(cur_node) };
                    cur_node = unsafe { handle.into_child_containing_index(&mut offset) };
                }
                self.leaf.write(cur_node);
                self.leaf_index = offset;
                return;
            }
            index = unsafe { parent.node_ptr().as_ref().parent_index.assume_init().into() };
            parent = unsafe {
                Node::<_, height::Positive, T>::new(
                    parent.node_ptr().as_ref().parent.unwrap_unchecked(),
                )
            };
            // (parent, index) = unsafe { parent.into_parent_and_index2().unwrap_unchecked() };
        }
    }

    pub fn move_next_inbounds_unchecked(&mut self) {
        let leaf_len = unsafe { LeafRef::new(self.leaf.assume_init()).len() };

        // fast path
        // TODO: why no over/underflow problems?
        self.leaf_index += 1;
        if self.leaf_index < leaf_len {
            return;
        }

        let mut index: usize = unsafe {
            self.leaf
                .assume_init()
                .as_ref()
                .parent_index
                .assume_init()
                .into()
        };
        let mut parent = unsafe {
            InternalMut::<T>::new(self.leaf.assume_init().as_ref().parent.unwrap_unchecked())
        };
        loop {
            if index + 1 < parent.len_children() {
                let mut cur_node = unsafe { *parent.children()[..].get_unchecked(index + 1) };
                let height = unsafe { cur_node.as_ref().height() };
                for _ in 0..height {
                    let mut handle = unsafe { InternalMut::new(cur_node) };
                    cur_node = unsafe { *handle.children()[..].get_unchecked(0) };
                }
                self.leaf.write(cur_node);
                self.leaf_index = 0;
                return;
            }
            index = unsafe { parent.node_ptr().as_ref().parent_index.assume_init().into() };
            parent = unsafe {
                Node::<_, height::Positive, T>::new(
                    parent.node_ptr().as_ref().parent.unwrap_unchecked(),
                )
            };
            // (parent, index) = unsafe { parent.into_parent_and_index2().unwrap_unchecked() };
        }
    }
    */

    fn tree(&self) -> &BVec<T> {
        unsafe { self.tree.as_ref() }
    }

    pub(crate) fn len(&self) -> usize {
        self.tree().len()
    }
}

impl<'a, T> CursorInner<'a, ownership::Immut<'a>, T> {
    #[must_use]
    pub unsafe fn get_unchecked(&self) -> &'a T {
        unsafe { LeafRef::new(self.leaf.assume_init()).value_unchecked(self.leaf_index) }
    }
}

impl<'a, T> CursorInner<'a, ownership::Mut<'a>, T> {
    fn root_mut(&mut self) -> &mut MaybeUninit<NodePtr<T>> {
        unsafe { &mut self.tree.as_mut().root }
    }

    #[must_use]
    pub unsafe fn get_unchecked(&self) -> &T {
        unsafe { LeafRef::new(self.leaf.assume_init()).value_unchecked(self.leaf_index) }
    }

    #[must_use]
    pub unsafe fn get_unchecked_mut(&mut self) -> &mut T {
        unsafe { LeafMut::new(self.leaf.assume_init()).into_value_unchecked_mut(self.leaf_index) }
    }

    #[must_use]
    pub unsafe fn into_unchecked_mut(self) -> &'a mut T {
        unsafe { LeafMut::new(self.leaf.assume_init()).into_value_unchecked_mut(self.leaf_index) }
    }

    /*
    /// # Panics
    /// panics if pointing past the end
    pub fn remove(&mut self) -> T {
        // if self.index >= self.tree.len() {
        //     panic!("index out of bounds");
        // }
        let mut leaf = self
            .leaf_mut()
            .expect("attempting to remove from empty tree");
        let mut leaf_index = self.leaf_index;
        assert!(leaf_index < leaf.len(), "out of bounds");

        unsafe { self.add_path_lengths_wrapping(1_usize.wrapping_neg()) };

        let ret = leaf.remove_child(leaf_index);

        let leaf_underfull = leaf.is_underfull();
        let leaf_is_empty = leaf.len() == 0;

        let Some((mut parent, mut self_index)) = leaf.into_parent_and_index3() else {
            // height is 1
            if leaf_is_empty {
                unsafe { Leaf::new(self.tree().root.assume_init()).free() };
            }
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

            let mut parent = parent.into_parent_and_index();

            // merge nodes if needed
            while let Some((mut parent_node, cur_index)) = parent {
                if !parent_node.maybe_handle_underfull_child(cur_index) {
                    break;
                }
                parent = parent_node.into_parent_and_index();
            }
        }

        // TODO: maybe don't do this here
        // move cursor to start of next leaf if pointing past the end of the current leaf
        if self.leaf().unwrap().len() == leaf_index {
            self.move_(0);
        }

        // move the root one level lower if needed
        unsafe {
            let mut old_root = Internal::new(self.tree().root().unwrap());

            if old_root.is_singleton() {
                let mut new_root = old_root.children().remove(0);
                old_root.free();
                self.root_mut().write(new_root);
            }
        }

        ret
    }*/
}

pub struct InboundsCursor<'a, T> {
    inner: CursorInner<'a, ownership::Immut<'a>, T>,
}

impl<'a, T> InboundsCursor<'a, T> {
    pub(crate) fn try_new(tree: &'a BVec<T>, index: usize) -> Option<Self> {
        CursorInner::try_new_inbounds(tree, index).map(|inner| Self { inner })
    }

    pub(crate) fn try_new_first(tree: &'a BVec<T>) -> Option<Self> {
        Some(Self {
            inner: CursorInner::try_new_inbounds_first(tree)?,
        })
    }

    pub(crate) fn try_new_last(tree: &'a BVec<T>) -> Option<Self> {
        tree.is_not_empty().then(|| unsafe {
            Self {
                inner: CursorInner::new_last_unchecked(tree),
            }
        })
    }
}

impl<'a, T> InboundsCursor<'a, T> {
    #[must_use]
    pub fn get(self) -> &'a T {
        unsafe { self.inner.get_unchecked() }
    }
}

pub struct InboundsCursorMut<'a, T> {
    inner: CursorInner<'a, ownership::Mut<'a>, T>,
}

impl<'a, T> InboundsCursorMut<'a, T> {
    pub(crate) fn try_new(tree: &'a mut BVec<T>, index: usize) -> Option<Self> {
        CursorInner::try_new_inbounds(tree, index).map(|inner| Self { inner })
    }

    pub(crate) fn try_new_first(tree: &'a mut BVec<T>) -> Option<Self> {
        Some(Self {
            inner: CursorInner::try_new_inbounds_first(tree)?,
        })
    }

    pub(crate) fn try_new_last(tree: &'a mut BVec<T>) -> Option<Self> {
        tree.is_not_empty().then(|| unsafe {
            Self {
                inner: CursorInner::new_last_unchecked(tree),
            }
        })
    }
}

impl<'a, T> InboundsCursorMut<'a, T> {
    pub fn into_mut(self) -> &'a mut T {
        unsafe { self.inner.into_unchecked_mut() }
    }
}
*/
