use core::{
    mem::MaybeUninit,
    ops::RangeFrom,
    ptr::{self, addr_of_mut, NonNull},
};

use alloc::boxed::Box;

use crate::{
    node::{InternalNode, LeafNode, NodePtr, RawNodeWithLen},
    utils::ArrayVecMut,
};

pub type Leaf<'a, T, const B: usize, const C: usize> =
    Node<ownership::Immut<'a>, height::Zero, T, B, C>;

impl<'a, T: 'a, const B: usize, const C: usize> Leaf<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new(node: NodePtr<T, B, C>) -> Self {
        let this = Self {
            node: unsafe { node.cast().as_ref() },
        };
        debug_assert!(this.len() <= C);
        this
    }

    pub fn value(&self, index: usize) -> Option<&'a T> {
        (index < self.len()).then(|| unsafe { self.value_unchecked(index) })
    }

    pub unsafe fn value_unchecked(&self, index: usize) -> &'a T {
        debug_assert!(self.len() <= C);
        debug_assert!(index < self.len());

        // We own a shared reference to this leaf, so there
        // should not be any mutable references which
        // could cause aliasing problems with taking
        // a reference to the whole array.
        unsafe { self.node.values.get_unchecked(index).assume_init_ref() }
    }
}

pub type Internal<'a, T, const B: usize, const C: usize> =
    Node<ownership::Immut<'a>, height::Positive, T, B, C>;

impl<'a, T: 'a, const B: usize, const C: usize> Internal<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(node: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { node.cast().as_ref() },
        }
    }

    pub unsafe fn child_containing_index(&self, index: &mut usize) -> NodePtr<T, B, C> {
        for (i, len) in self.node.lengths.iter().enumerate() {
            match index.checked_sub(unsafe { len.assume_init() }) {
                Some(r) => *index = r,
                None => return unsafe { self.node.children[i].assume_init() },
            }
        }

        panic!();
    }
}

pub mod height {
    use core::marker::PhantomData;

    use crate::node::{InternalNode, LeafNode};

    #[derive(Copy, Clone)]
    pub struct Zero;
    #[derive(Copy, Clone)]
    pub struct Positive;
    #[derive(Copy, Clone)]
    pub struct One;
    #[derive(Copy, Clone)]
    pub struct TwoOrMore;
    pub struct BrandedTwoOrMore<'id>(PhantomData<*mut &'id ()>);
    #[derive(Copy, Clone)]
    pub struct ChildOf<H>(PhantomData<H>);

    pub unsafe trait Internal {}
    unsafe impl<'id> Internal for ChildOf<BrandedTwoOrMore<'id>> {}
    unsafe impl<'id> Internal for BrandedTwoOrMore<'id> {}
    unsafe impl Internal for Positive {}
    unsafe impl Internal for One {}
    unsafe impl Internal for TwoOrMore {}

    pub unsafe trait ExactInternal: Internal + Sized {
        type ChildHeight;
        unsafe fn make_child_height(&self) -> Self::ChildHeight;
    }
    unsafe impl<'id> ExactInternal for BrandedTwoOrMore<'id> {
        type ChildHeight = ChildOf<Self>;
        unsafe fn make_child_height(&self) -> Self::ChildHeight {
            ChildOf(PhantomData)
        }
    }
    unsafe impl<'id> ExactInternal for ChildOf<BrandedTwoOrMore<'id>> {
        type ChildHeight = ChildOf<Self>;
        unsafe fn make_child_height(&self) -> Self::ChildHeight {
            ChildOf(PhantomData)
        }
    }
    unsafe impl ExactInternal for One {
        type ChildHeight = Zero;
        unsafe fn make_child_height(&self) -> Self::ChildHeight {
            Zero
        }
    }

    pub unsafe trait Height {
        type NodeType<T, const B: usize, const C: usize>;
    }

    unsafe impl Height for Zero {
        type NodeType<T, const B: usize, const C: usize> = LeafNode<T, B, C>;
    }

    unsafe impl<H: Internal> Height for H {
        type NodeType<T, const B: usize, const C: usize> = InternalNode<T, B, C>;
    }
}

pub mod ownership {
    use core::{marker::PhantomData, ptr::NonNull};

    use alloc::boxed::Box;

    // TODO: should this be covariant?
    pub struct Immut<'a>(PhantomData<&'a ()>);

    pub struct Mut<'a>(PhantomData<&'a mut ()>);
    pub struct Owned;

    pub unsafe trait Mutable {}
    unsafe impl<'a> Mutable for Mut<'a> {}
    unsafe impl Mutable for Owned {}

    pub unsafe trait Reference {}
    unsafe impl<'a> Reference for Immut<'a> {}
    unsafe impl<'a> Reference for Mut<'a> {}

    pub unsafe trait NodePtrType<T> {
        type Ptr;
        unsafe fn from_raw(ptr: NonNull<T>) -> Self::Ptr;
        fn as_raw(this: &mut Self::Ptr) -> NonNull<T>;
        fn as_ref(this: &Self::Ptr) -> &T;
    }

    unsafe impl<'a, T: 'a> NodePtrType<T> for Immut<'a> {
        type Ptr = &'a T;
        unsafe fn from_raw(ptr: NonNull<T>) -> Self::Ptr {
            unsafe { ptr.as_ref() }
        }
        fn as_raw(this: &mut Self::Ptr) -> NonNull<T> {
            NonNull::from(*this)
        }
        fn as_ref(this: &Self::Ptr) -> &T {
            this
        }
    }

    unsafe impl<'a, T: 'a> NodePtrType<T> for Mut<'a> {
        type Ptr = NonNull<T>;
        unsafe fn from_raw(ptr: NonNull<T>) -> Self::Ptr {
            ptr
        }
        fn as_raw(this: &mut Self::Ptr) -> NonNull<T> {
            *this
        }
        fn as_ref(this: &Self::Ptr) -> &T {
            unsafe { this.as_ref() }
        }
    }

    unsafe impl<T> NodePtrType<T> for Owned {
        type Ptr = Box<T>;
        unsafe fn from_raw(ptr: NonNull<T>) -> Self::Ptr {
            unsafe { Box::from_raw(ptr.as_ptr()) }
        }
        fn as_raw(this: &mut Self::Ptr) -> NonNull<T> {
            NonNull::from(&mut **this)
        }
        fn as_ref(this: &Self::Ptr) -> &T {
            this
        }
    }
}

pub struct Node<O, H, T, const B: usize, const C: usize>
where
    H: height::Height,
    O: ownership::NodePtrType<H::NodeType<T, B, C>>,
{
    node: O::Ptr,
}

pub type LeafMut<'a, T, const B: usize, const C: usize> =
    Node<ownership::Mut<'a>, height::Zero, T, B, C>;

pub type OwnedNode<H, T, const B: usize, const C: usize> = Node<ownership::Owned, H, T, B, C>;

impl<H, T, const B: usize, const C: usize> OwnedNode<H, T, B, C>
where
    H: height::Height + Copy,
{
    pub fn as_mut(&mut self) -> Node<ownership::Mut, H, T, B, C> {
        Node {
            node: NonNull::from(&mut *self.node),
        }
    }
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Zero, T, B, C> {
    pub unsafe fn new_leaf(node: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { Box::from_raw(node.cast().as_ptr()) },
        }
    }

    pub fn new_empty_leaf() -> Self {
        let node = LeafNode::<T, B, C>::new();
        Self {
            node: unsafe { Box::from_raw(node.as_ptr().cast()) },
        }
    }
}

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    O: ownership::NodePtrType<H::NodeType<T, B, C>> + ownership::NodePtrType<InternalNode<T, B, C>>,
    H: height::Height,
{
    pub fn node_ptr(&mut self) -> NodePtr<T, B, C> {
        <O as ownership::NodePtrType<H::NodeType<T, B, C>>>::as_raw(&mut self.node).cast()
    }

    pub unsafe fn into_parent_and_index2(
        mut self,
    ) -> Option<(Node<O, height::Positive, T, B, C>, usize)>
    where
        O: ownership::Reference,
    {
        unsafe {
            let parent = Node::new_internal((*self.node_ptr().as_ptr()).parent?);
            Some((
                parent,
                (*self.node_ptr().as_ptr())
                    .parent_index
                    .assume_init()
                    .into(),
            ))
        }
    }
}

impl<O, T, const B: usize, const C: usize> Node<O, height::Zero, T, B, C>
where
    O: ownership::NodePtrType<LeafNode<T, B, C>>,
{
    pub fn len(&self) -> usize {
        usize::from(O::as_ref(&self.node).base.children_len)
    }
}

impl<'a, T: 'a, const B: usize, const C: usize> LeafMut<'a, T, B, C> {
    fn leaf_ptr(&mut self) -> *mut LeafNode<T, B, C> {
        self.node_ptr().cast().as_ptr()
    }

    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new_leaf(node: NodePtr<T, B, C>) -> LeafMut<'a, T, B, C> {
        // debug_assert!(unsafe { (*node).len() <= C });

        Self { node: node.cast() }
    }

    pub fn values_mut(&mut self) -> ArrayVecMut<T, C> {
        unsafe {
            ArrayVecMut::new(
                addr_of_mut!((*self.leaf_ptr()).values),
                addr_of_mut!((*self.leaf_ptr()).base.children_len).cast(),
            )
        }
    }

    pub unsafe fn into_value_unchecked_mut(mut self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= C);
        debug_assert!(index < len);
        unsafe {
            (*self.leaf_ptr())
                .values
                .as_mut()
                .get_unchecked_mut(index)
                .assume_init_mut()
        }
    }

    fn is_full(&self) -> bool {
        self.len() == C
    }

    pub fn insert_value(&mut self, index: usize, value: T) -> Option<SplitResult<T, B, C>> {
        assert!(index <= self.len());

        if self.is_full() {
            Some(if index <= C / 2 {
                SplitResult::Left(self.split_and_insert_left(index, value))
            } else {
                SplitResult::Right(self.split_and_insert_right(index, value))
            })
        } else {
            self.values_mut().insert(index, value);
            None
        }
    }

    fn split_and_insert_left(&mut self, index: usize, value: T) -> RawNodeWithLen<T, B, C> {
        let split_index = C / 2;
        let mut new_node = OwnedNode::<_, _, B, C>::new_empty_leaf();
        let mut new_leaf = new_node.as_mut();
        self.values_mut().split(split_index, new_leaf.values_mut());
        self.values_mut().insert(index, value);
        RawNodeWithLen(
            new_leaf.values_mut().len(),
            NonNull::from(Box::leak(new_node.node)).cast(),
        )
    }

    fn split_and_insert_right(&mut self, index: usize, value: T) -> RawNodeWithLen<T, B, C> {
        let split_index = (C - 1) / 2 + 1;
        let mut new_node = OwnedNode::<_, _, B, C>::new_empty_leaf();
        let mut new_leaf = new_node.as_mut();
        self.values_mut().split(split_index, new_leaf.values_mut());
        new_leaf.values_mut().insert(index - self.len(), value);
        RawNodeWithLen(
            new_leaf.values_mut().len(),
            NonNull::from(Box::leak(new_node.node)).cast(),
        )
    }
}

impl<O, T, const B: usize, const C: usize> Node<O, height::One, T, B, C>
where
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    pub unsafe fn new_parent_of_leaf(node: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { O::from_raw(node.cast()) },
        }
    }

    pub fn child_mut(&mut self, index: usize) -> LeafMut<T, B, C> {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        unsafe { LeafMut::new_leaf(ptr.add(index).read().assume_init()) }
    }

    pub fn child_pair_at(&mut self, index: usize) -> [LeafMut<T, B, C>; 2] {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        [
            unsafe { LeafMut::new_leaf(ptr.add(index).read().assume_init()) },
            unsafe { LeafMut::new_leaf(ptr.add(index + 1).read().assume_init()) },
        ]
    }

    pub fn handle_underfull_leaf_child_head(&mut self) {
        let [mut cur, mut next] = self.child_pair_at(0);

        if next.is_almost_underfull() {
            cur.append_children(next);
            let next_len = unsafe { (*self.internal_ptr()).lengths[1].assume_init() };
            unsafe {
                *(*self.internal_ptr()).lengths[0].assume_init_mut() += next_len;
                OwnedNode::new_leaf(self.remove_child(1).1).free();
            }
        } else {
            cur.push_back_child(next.pop_front_child());
            unsafe {
                *(*self.internal_ptr()).lengths[0].assume_init_mut() += 1;
                *(*self.internal_ptr()).lengths[1].assume_init_mut() -= 1;
            }
        }
    }

    pub fn handle_underfull_leaf_child_tail(&mut self, index: &mut usize, child_index: &mut usize) {
        let [mut prev, mut cur] = self.child_pair_at(*index - 1);

        if prev.is_almost_underfull() {
            let prev_children_len = prev.len_children();
            prev.append_children(cur);
            let cur_len = unsafe { (*self.internal_ptr()).lengths[*index].assume_init() };
            unsafe {
                *(*self.internal_ptr()).lengths[*index - 1].assume_init_mut() += cur_len;
                OwnedNode::new_leaf(self.remove_child(*index).1).free();
            }
            *index -= 1;
            *child_index += prev_children_len;
        } else {
            cur.push_front_child(prev.pop_back_child());
            unsafe {
                *(*self.internal_ptr()).lengths[*index - 1].assume_init_mut() -= 1;
                *(*self.internal_ptr()).lengths[*index].assume_init_mut() += 1;
            }
            *child_index += 1;
        }
    }
}

impl<O, T, const B: usize, const C: usize> Node<O, height::TwoOrMore, T, B, C>
where
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    pub unsafe fn new_parent_of_internal(node: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { O::from_raw(node.cast()) },
        }
    }

    pub fn with_brand<'a, F, R>(&'a mut self, f: F) -> R
    where
        T: 'a,
        F: for<'new_id> FnOnce(
            Node<ownership::Mut<'a>, height::BrandedTwoOrMore<'new_id>, T, B, C>,
        ) -> R,
    {
        let parent = Node {
            node: O::as_raw(&mut self.node),
        };

        f(parent)
    }

    pub fn maybe_handle_underfull_child(&mut self, index: usize) -> bool {
        self.with_brand(|mut parent| {
            let is_child_underfull = parent.child_mut(index).is_underfull();

            if is_child_underfull {
                if index > 0 {
                    parent.handle_underfull_internal_child_tail(index);
                } else {
                    parent.handle_underfull_internal_child_head();
                }
            }

            is_child_underfull
        })
    }
}

impl<'id, O, T, const B: usize, const C: usize> Node<O, height::BrandedTwoOrMore<'id>, T, B, C>
where
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    pub fn child_mut(&mut self, index: usize) -> Node<ownership::Mut, height::Positive, T, B, C> {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        Node {
            node: unsafe { ptr.add(index).read().assume_init().cast() },
        }
    }

    pub fn child_pair_at(
        &mut self,
        index: usize,
    ) -> [Node<ownership::Mut, height::ChildOf<height::BrandedTwoOrMore<'id>>, T, B, C>; 2] {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        [
            Node {
                node: unsafe { ptr.add(index).read().assume_init().cast() },
            },
            Node {
                node: unsafe { ptr.add(index + 1).read().assume_init().cast() },
            },
        ]
    }

    fn handle_underfull_internal_child_head(&mut self) {
        let [mut cur, mut next] = self.child_pair_at(0);

        if next.is_almost_underfull() {
            cur.append_children(next);
            unsafe {
                let next_len = (*self.internal_ptr()).lengths[1].assume_init();
                *(*self.internal_ptr()).lengths[0].assume_init_mut() += next_len;
                OwnedNode::new_internal_owned(self.remove_child(1).1).free();
            }
        } else {
            let x = next.pop_front_child();
            let x_len = x.0;
            cur.push_back_child(x);
            unsafe {
                *(*self.internal_ptr()).lengths[0].assume_init_mut() += x_len;
                *(*self.internal_ptr()).lengths[1].assume_init_mut() -= x_len;
            }
        }
    }

    fn handle_underfull_internal_child_tail(&mut self, index: usize) {
        let [mut prev, mut cur] = self.child_pair_at(index - 1);

        if prev.is_almost_underfull() {
            prev.append_children(cur);
            let cur_len = unsafe { (*self.internal_ptr()).lengths[index].assume_init() };
            unsafe {
                *(*self.internal_ptr()).lengths[index - 1].assume_init_mut() += cur_len;
                OwnedNode::new_internal_owned(self.remove_child(index).1).free();
            }
        } else {
            let x = prev.pop_back_child();
            let x_len = x.0;
            cur.push_front_child(x);
            unsafe {
                *(*self.internal_ptr()).lengths[index - 1].assume_init_mut() -= x_len;
                *(*self.internal_ptr()).lengths[index].assume_init_mut() += x_len;
            }
        }
    }
}

pub trait FreeableNode {
    fn free(self);
}

impl<T, const B: usize, const C: usize> FreeableNode for OwnedNode<height::Zero, T, B, C> {
    fn free(self) {
        debug_assert_eq!(self.node.base.children_len, 0);
        unsafe { Box::from_raw(Box::into_raw(self.node).cast::<LeafNode<T, B, C>>()) };
    }
}

impl<H, T, const B: usize, const C: usize> FreeableNode for OwnedNode<H, T, B, C>
where
    H: height::Internal,
{
    fn free(self) {
        debug_assert_eq!(self.node.base.children_len, 0);
        // debug_assert_eq!(self.node.len(), 0);
        unsafe { Box::from_raw(Box::into_raw(self.node).cast::<InternalNode<T, B, C>>()) };
    }
}

pub unsafe trait ExactHeightNode {
    type Child;
    const UNDERFULL_LEN: usize;
    fn len_children(&self) -> usize;
    fn insert_child(&mut self, index: usize, child: Self::Child);
    fn remove_child(&mut self, index: usize) -> Self::Child;
    fn append_children(&mut self, other: Self);

    fn push_front_child(&mut self, child: Self::Child) {
        self.insert_child(0, child);
    }
    fn push_back_child(&mut self, child: Self::Child) {
        self.insert_child(self.len_children(), child);
    }
    fn pop_front_child(&mut self) -> Self::Child {
        self.remove_child(0)
    }
    fn pop_back_child(&mut self) -> Self::Child {
        self.remove_child(self.len_children() - 1)
    }
    fn is_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN
    }
    fn is_almost_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN + 1
    }
}

unsafe impl<'a, T: 'a, const B: usize, const C: usize> ExactHeightNode for LeafMut<'a, T, B, C> {
    type Child = T;
    const UNDERFULL_LEN: usize = (C - 1) / 2;
    fn len_children(&self) -> usize {
        self.len()
    }
    fn insert_child(&mut self, index: usize, child: Self::Child) {
        self.values_mut().insert(index, child);
    }
    fn remove_child(&mut self, index: usize) -> Self::Child {
        self.values_mut().remove(index)
    }
    fn append_children(&mut self, mut other: Self) {
        self.values_mut().append(other.values_mut());
    }
}

unsafe impl<O, H, T, const B: usize, const C: usize> ExactHeightNode for Node<O, H, T, B, C>
where
    H: height::ExactInternal,
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    type Child = RawNodeWithLen<T, B, C>;
    const UNDERFULL_LEN: usize = (B - 1) / 2;
    fn len_children(&self) -> usize {
        self.len_children()
    }
    fn insert_child(&mut self, index: usize, child: Self::Child) {
        self.as_array_vec().insert(index, child.1);
        unsafe {
            let lens_ptr = (*self.internal_ptr()).lengths.as_mut_ptr();
            ptr::copy(
                lens_ptr.add(index),
                lens_ptr.add(index + 1),
                self.len_children() - 1 - index,
            );
            ptr::write(lens_ptr.add(index), MaybeUninit::new(child.0));
        }
        self.set_parent_links(index..);
    }
    fn remove_child(&mut self, index: usize) -> Self::Child {
        let node = self.as_array_vec().remove(index);
        let node_len;
        unsafe {
            let lens_ptr = (*self.internal_ptr()).lengths.as_mut_ptr();
            node_len = lens_ptr.add(index).read().assume_init();
            ptr::copy(
                lens_ptr.add(index + 1),
                lens_ptr.add(index),
                self.len_children() - index,
            );
        }
        self.set_parent_links(index..);
        RawNodeWithLen(node_len, node)
    }
    fn append_children(&mut self, mut other: Self) {
        let self_old_len = self.len_children();
        unsafe {
            let lens_src = (*other.internal_ptr()).lengths.as_ptr();
            let lens_dst = (*self.internal_ptr())
                .lengths
                .as_mut_ptr()
                .add(self.as_array_vec().len());
            ptr::copy_nonoverlapping(lens_src, lens_dst, other.as_array_vec().len());
        }
        self.as_array_vec().append(other.as_array_vec());
        self.set_parent_links(self_old_len..);
    }
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Positive, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new_internal_owned(node: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { Box::from_raw(node.as_ptr().cast()) },
        }
    }
}

impl<O, T, const B: usize, const C: usize> Node<O, height::Positive, T, B, C>
where
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new_internal(node: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { O::from_raw(node.cast()) },
        }
    }
}

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Internal,
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    pub const UNDERFULL_LEN: usize = (B - 1) / 2;

    pub fn internal_ptr(&mut self) -> *mut InternalNode<T, B, C> {
        O::as_raw(&mut self.node).cast().as_ptr()
    }

    pub fn len_children(&self) -> usize {
        usize::from(O::as_ref(&self.node).base.children_len)
    }

    pub fn is_singleton(&self) -> bool {
        self.len_children() == 1
    }

    fn is_full(&self) -> bool {
        self.len_children() == B
    }

    pub fn is_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN
    }

    pub fn as_array_vec(&mut self) -> ArrayVecMut<NodePtr<T, B, C>, B> {
        unsafe {
            ArrayVecMut::new(
                addr_of_mut!((*self.internal_ptr()).children),
                addr_of_mut!((*O::as_raw(&mut self.node).as_ptr()).base.children_len).cast(),
            )
        }
    }

    pub unsafe fn into_parent_and_index(
        mut self,
    ) -> Option<(Node<O, height::TwoOrMore, T, B, C>, usize)>
    where
        O: ownership::Reference,
    {
        unsafe {
            let parent =
                Node::new_parent_of_internal((*O::as_raw(&mut self.node).as_ptr()).base.parent?);
            Some((
                parent,
                (*O::as_raw(&mut self.node).as_ptr())
                    .base
                    .parent_index
                    .assume_init()
                    .into(),
            ))
        }
    }

    pub fn into_internal(self) -> Node<O, height::Positive, T, B, C> {
        Node { node: self.node }
    }

    pub unsafe fn into_child_containing_index(mut self, index: &mut usize) -> NodePtr<T, B, C> {
        // debug_assert!(*index < self.len());
        unsafe {
            for (i, len) in &mut (*self.internal_ptr()).lengths.iter().enumerate() {
                match index.checked_sub(len.assume_init()) {
                    Some(r) => *index = r,
                    None => return (*self.internal_ptr()).children[i].assume_init(),
                }
            }
        }

        panic!();
    }

    pub unsafe fn insert_split_of_child(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T, B, C>,
    ) -> Option<RawNodeWithLen<T, B, C>> {
        unsafe {
            *(*self.internal_ptr()).lengths[index].assume_init_mut() -= node.0;
            if self.is_full() {
                use core::cmp::Ordering::Less;
                Some(match index.cmp(&Self::UNDERFULL_LEN) {
                    Less => self.split_and_insert_left(index + 1, node),
                    _ => self.split_and_insert_right(index + 1, node),
                })
            } else {
                self.insert_fitting(index + 1, node);
                None
            }
        }
    }

    unsafe fn insert_fitting(&mut self, index: usize, node: RawNodeWithLen<T, B, C>) {
        debug_assert!(!self.is_full());
        self.as_array_vec().insert(index, node.1);
        unsafe {
            let lens_ptr = (*self.internal_ptr()).lengths.as_mut_ptr();
            ptr::copy(
                lens_ptr.add(index),
                lens_ptr.add(index + 1),
                self.len_children() - 1 - index,
            );
            ptr::write(lens_ptr.add(index), MaybeUninit::new(node.0));
        }
        self.set_parent_links(index..);
    }

    unsafe fn split_and_insert_left(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T, B, C>,
    ) -> RawNodeWithLen<T, B, C> {
        let split_index = Self::UNDERFULL_LEN;

        let new_sibling_node = InternalNode::<T, B, C>::new();
        let mut new_sibling: Node<ownership::Mut, _, _, B, C> =
            unsafe { Node::new_internal(new_sibling_node.cast()) };

        unsafe {
            let tail_len = self.len_children() - split_index;
            let lens_ptr = (*self.internal_ptr()).lengths.as_ptr();
            ptr::copy_nonoverlapping(
                lens_ptr.add(split_index),
                (*new_sibling_node.as_ptr()).lengths.as_mut_ptr(),
                tail_len,
            );
        }
        self.as_array_vec()
            .split(split_index, new_sibling.as_array_vec());
        unsafe { self.insert_fitting(index, node) };

        new_sibling.set_parent_links(0..);
        RawNodeWithLen(new_sibling.sum_lens(), new_sibling_node.cast())
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T, B, C>,
    ) -> RawNodeWithLen<T, B, C> {
        let split_index = Self::UNDERFULL_LEN + 1;

        let new_sibling_node = InternalNode::<T, B, C>::new();
        let mut new_sibling: Node<ownership::Mut, _, _, B, C> =
            unsafe { Node::new_internal(new_sibling_node.cast()) };

        unsafe {
            let tail_len = self.len_children() - split_index;
            let lens_ptr = (*self.internal_ptr()).lengths.as_ptr();
            ptr::copy_nonoverlapping(
                lens_ptr.add(split_index),
                (*new_sibling_node.as_ptr()).lengths.as_mut_ptr(),
                tail_len,
            );
        }
        self.as_array_vec()
            .split(split_index, new_sibling.as_array_vec());
        unsafe {
            new_sibling.insert_fitting(index - split_index, node);
        }

        new_sibling.set_parent_links(0..);
        RawNodeWithLen(new_sibling.sum_lens(), new_sibling_node.cast())
    }

    fn set_parent_links(&mut self, range: RangeFrom<usize>) {
        for (i, n) in self.as_array_vec()[range.clone()].iter_mut().enumerate() {
            unsafe {
                (*n.as_ptr()).parent = Some(O::as_raw(&mut self.node).cast());
                (*n.as_ptr()).parent_index.write((i + range.start) as u16);
            }
        }
    }

    pub fn sum_lens(&mut self) -> usize {
        unsafe {
            (*self.internal_ptr())
                .lengths
                .iter()
                .take(self.len_children())
                .map(|l| l.assume_init())
                .sum()
        }
    }
}

pub enum SplitResult<T, const B: usize, const C: usize> {
    Left(RawNodeWithLen<T, B, C>),
    Right(RawNodeWithLen<T, B, C>),
}
