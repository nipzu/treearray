use core::{hint::unreachable_unchecked, mem::MaybeUninit};

use alloc::boxed::Box;

use crate::{
    node::{Children, Node},
    utils::ArrayVecMut,
};

pub struct Leaf<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Leaf<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    pub const fn len(&self) -> usize {
        self.node.len()
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
        unsafe {
            self.node
                .ptr
                .values
                .as_ref()
                .get_unchecked(index)
                .assume_init_ref()
        }
    }
}

pub struct Internal<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Internal<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        Self { node }
    }

    pub fn children(&self) -> &'a Children<T, B, C> {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node.ptr.children.as_ref() }
    }

    pub unsafe fn child_containing_index(&self, index: &mut usize) -> &'a Node<T, B, C> {
        for child in self.children().children() {
            let len = child.len();
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return child,
            }
        }

        unsafe { unreachable_unchecked() };
    }

    pub unsafe fn index_of_child_ptr(&self, child_ptr: *const Node<T, B, C>) -> usize {
        let slice_ptr = unsafe { self.node.ptr.children.as_ptr() };
        #[allow(clippy::cast_sign_loss)]
        unsafe {
            child_ptr.offset_from(slice_ptr.cast()) as usize
        }
    }
}

pub mod height {
    use core::{marker::PhantomData, num::NonZeroUsize};

    #[derive(Copy, Clone)]
    pub struct Zero;
    #[derive(Copy, Clone)]
    pub struct Positive;
    #[derive(Copy, Clone)]
    pub struct One;
    #[derive(Copy, Clone)]
    pub struct TwoOrMore;
    #[derive(Copy, Clone)]
    pub struct DynamicInternal(NonZeroUsize);
    #[derive(Copy, Clone)]
    pub struct BrandedTwoOrMore<'id>(PhantomData<*mut &'id ()>);
    #[derive(Copy, Clone)]
    pub struct ChildOf<H>(PhantomData<H>);

    impl<'id> BrandedTwoOrMore<'id> {
        pub const fn new() -> Self {
            Self(PhantomData)
        }
    }

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
}

pub struct OwnedNode<H, T, const B: usize, const C: usize> {
    node: Node<T, B, C>,
    height: H,
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Positive, T, B, C> {
    pub const unsafe fn new_internal(node: Node<T, B, C>) -> Self {
        Self {
            node,
            height: height::Positive,
        }
    }
}

impl<H, T, const B: usize, const C: usize> OwnedNode<H, T, B, C> {
    pub fn as_mut(&mut self) -> NodeMut<H, T, B, C>
    where
        H: Copy,
    {
        NodeMut {
            node: &mut self.node,
            height: self.height,
        }
    }
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Zero, T, B, C> {
    pub const unsafe fn new_leaf(node: Node<T, B, C>) -> Self {
        Self {
            node,
            height: height::Zero,
        }
    }
}

pub struct NodeMut<'a, H, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
    height: H,
}

pub type LeafMut<'a, T, const B: usize, const C: usize> = NodeMut<'a, height::Zero, T, B, C>;

pub type InternalMut<'a, T, const B: usize, const C: usize> =
    NodeMut<'a, height::Positive, T, B, C>;

impl<'a, T, const B: usize, const C: usize, H> NodeMut<'a, H, T, B, C> {
    pub fn len(&self) -> usize {
        self.node.len()
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.node.length = new_len;
    }

    pub fn into_node(self) -> &'a mut Node<T, B, C> {
        self.node
    }
}

impl<'a, T, const B: usize, const C: usize> LeafMut<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new_leaf(node: &'a mut Node<T, B, C>) -> NodeMut<'a, height::Zero, T, B, C> {
        debug_assert!(node.len() <= C);

        NodeMut {
            node,
            height: height::Zero,
        }
    }

    pub fn values_mut(&mut self) -> ArrayVecMut<T, C> {
        unsafe { ArrayVecMut::new(self.node.ptr.values.as_mut(), &mut self.node.length) }
    }

    pub fn values_maybe_uninit_mut(&mut self) -> &mut [MaybeUninit<T>; C] {
        unsafe { self.node.ptr.values.as_mut() }
    }

    pub unsafe fn into_value_unchecked_mut(self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= C);
        debug_assert!(index < len);
        unsafe {
            self.node
                .ptr
                .values
                .as_mut()
                .get_unchecked_mut(index)
                .assume_init_mut()
        }
    }

    fn is_full(&self) -> bool {
        self.len() == C
    }

    pub fn insert_value(&mut self, index: usize, value: T) -> InsertResult<T, B, C> {
        assert!(index <= self.len());

        if self.is_full() {
            InsertResult::Split(if index <= C / 2 {
                SplitResult::Left(self.split_and_insert_left(index, value))
            } else {
                SplitResult::Right(self.split_and_insert_right(index, value))
            })
        } else {
            self.values_mut().insert(index, value);
            InsertResult::Fit
        }
    }

    fn split_and_insert_left(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let split_index = C / 2;
        let mut new_node = Node::empty_leaf();
        let mut new_leaf = unsafe { NodeMut::new_leaf(&mut new_node) };
        let mut values = self.values_mut();
        values.split(split_index, new_leaf.values_mut());
        values.insert(index, value);
        new_node
    }

    fn split_and_insert_right(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let split_index = (C - 1) / 2 + 1;
        let mut new_node = Node::empty_leaf();
        let mut new_leaf = unsafe { NodeMut::new_leaf(&mut new_node) };
        self.values_mut().split(split_index, new_leaf.values_mut());
        new_leaf.values_mut().insert(index - self.len(), value);
        new_node
    }
}

impl<'a, T, const B: usize, const C: usize> NodeMut<'a, height::One, T, B, C> {
    pub unsafe fn new_parent_of_leaf(node: &'a mut Node<T, B, C>) -> Self {
        Self {
            node,
            height: height::One,
        }
    }
}

impl<'a, T, const B: usize, const C: usize> NodeMut<'a, height::TwoOrMore, T, B, C> {
    pub unsafe fn new_parent_of_internal(node: &'a mut Node<T, B, C>) -> Self {
        Self {
            node,
            height: height::TwoOrMore,
        }
    }

    pub fn with_brand<F, R>(&mut self, f: F) -> R
    where
        F: for<'new_id> FnOnce(NodeMut<height::BrandedTwoOrMore<'new_id>, T, B, C>) -> R,
    {
        let parent = NodeMut {
            node: self.node,
            height: height::BrandedTwoOrMore::new(),
        };

        f(parent)
    }
}

pub trait ApproximateHeightNode {
    fn free(self);
}

impl<T, const B: usize, const C: usize> ApproximateHeightNode for OwnedNode<height::Zero, T, B, C> {
    fn free(mut self) {
        debug_assert_eq!(self.as_mut().len(), 0);
        unsafe { Box::from_raw(self.as_mut().values_maybe_uninit_mut()) };
    }
}

impl<H, T, const B: usize, const C: usize> ApproximateHeightNode for OwnedNode<H, T, B, C>
where
    H: height::Internal + Copy,
{
    fn free(mut self) {
        debug_assert_eq!(self.as_mut().children().children().len(), 0);
        unsafe { Box::from_raw(self.as_mut().into_children_mut()) };
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

unsafe impl<'a, T, const B: usize, const C: usize> ExactHeightNode
    for NodeMut<'a, height::Zero, T, B, C>
{
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

unsafe impl<'a, H, T, const B: usize, const C: usize> ExactHeightNode for NodeMut<'a, H, T, B, C>
where
    H: height::ExactInternal,
{
    type Child = OwnedNode<H::ChildHeight, T, B, C>;
    const UNDERFULL_LEN: usize = (B - 1) / 2;
    fn len_children(&self) -> usize {
        self.count_children()
    }
    fn insert_child(&mut self, index: usize, child: Self::Child) {
        unsafe { self.set_len(self.len() + child.node.len()) };
        self.children_mut().insert(index, child.node);
    }
    fn remove_child(&mut self, index: usize) -> Self::Child {
        let node = self.children_mut().remove(index);
        unsafe { self.set_len(self.len() - node.len()) };
        OwnedNode {
            node,
            height: unsafe { self.height.make_child_height() },
        }
    }
    fn append_children(&mut self, mut other: Self) {
        self.children_mut().append(other.children_mut());

        unsafe { self.set_len(self.len() + other.len()) };
        unsafe { other.set_len(0) };
    }
}

impl<'a, H, T, const B: usize, const C: usize> NodeMut<'a, H, T, B, C>
where
    H: height::ExactInternal,
{
    pub fn child_pair_at(&mut self, index: usize) -> [NodeMut<H::ChildHeight, T, B, C>; 2] {
        let (h1, h2) = unsafe {
            (
                self.height.make_child_height(),
                self.height.make_child_height(),
            )
        };
        let (fst, snd) = self.raw_children_mut().pair_at(index);
        [
            NodeMut {
                node: fst,
                height: h1,
            },
            NodeMut {
                node: snd,
                height: h2,
            },
        ]
    }

    pub fn child_mut(&mut self, index: usize) -> NodeMut<H::ChildHeight, T, B, C> {
        let height = unsafe { self.height.make_child_height() };
        NodeMut {
            node: &mut self.raw_children_mut().children_mut()[index],
            height,
        }
    }

    pub fn handle_underfull_child_head(&mut self)
    where
        for<'b> NodeMut<'b, H::ChildHeight, T, B, C>: ExactHeightNode,
        OwnedNode<H::ChildHeight, T, B, C>: ApproximateHeightNode,
    {
        let [mut cur, mut next] = self.child_pair_at(0);

        if next.is_almost_underfull() {
            cur.append_children(next);
            self.remove_child(1).free();
        } else {
            cur.push_back_child(next.pop_front_child());
        }
    }

    pub fn handle_underfull_child_tail(&mut self, index: &mut usize, child_index: &mut usize)
    where
        for<'b> NodeMut<'b, H::ChildHeight, T, B, C>: ExactHeightNode,
        OwnedNode<H::ChildHeight, T, B, C>: ApproximateHeightNode,
    {
        let [mut prev, mut cur] = self.child_pair_at(*index - 1);

        if prev.is_almost_underfull() {
            let prev_len = prev.len_children();
            prev.append_children(cur);
            self.remove_child(*index).free();
            *index -= 1;
            *child_index += prev_len;
        } else {
            cur.push_front_child(prev.pop_back_child());
            *child_index += 1;
        }
    }
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        Self {
            node,
            height: height::Positive,
        }
    }
}

impl<'a, H, T, const B: usize, const C: usize> NodeMut<'a, H, T, B, C>
where
    H: height::Internal,
{
    pub const UNDERFULL_LEN: usize = (B - 1) / 2;

    pub fn count_children(&self) -> usize {
        self.children().len
    }

    pub fn is_singleton(&self) -> bool {
        self.children().len == 1
    }

    fn is_full(&self) -> bool {
        self.children().len == B
    }

    pub unsafe fn into_child_containing_index(self, index: &mut usize) -> &'a mut Node<T, B, C> {
        for child in self.into_children_mut().children_mut() {
            // let child = unsafe { child.assume_init_mut() };
            let len = child.len();
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return child,
            }
        }

        debug_assert!(false);
        unsafe { unreachable_unchecked() };
    }

    pub unsafe fn index_of_child_ptr(&self, elem_ptr: *const Node<T, B, C>) -> usize {
        let slice_ptr = unsafe { self.node.ptr.children.as_ptr() };
        #[allow(clippy::cast_sign_loss)]
        unsafe {
            elem_ptr.offset_from(slice_ptr.cast()) as usize
        }
    }

    pub fn children(&self) -> &Children<T, B, C> {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node.ptr.children.as_ref() }
    }

    pub fn children_mut(&mut self) -> ArrayVecMut<Node<T, B, C>, B> {
        self.raw_children_mut().as_array_vec()
    }

    pub fn raw_children_mut(&mut self) -> &mut Children<T, B, C> {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn into_children_mut(self) -> &'a mut Children<T, B, C> {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn raw_child_mut(&mut self, index: usize) -> &mut Node<T, B, C> {
        &mut self.raw_children_mut().children_mut()[index]
    }

    pub unsafe fn insert_node(
        &mut self,
        index: usize,
        prev_split_res: SplitResult<T, B, C>,
    ) -> InsertResult<T, B, C> {
        let (path_through_self, node) = match prev_split_res {
            SplitResult::Right(n) => (false, n),
            SplitResult::Left(n) => (true, n),
        };
        unsafe {
            if self.is_full() {
                use core::cmp::Ordering::{Equal, Less};
                InsertResult::Split(match index.cmp(&(Self::UNDERFULL_LEN + 1)) {
                    Less => SplitResult::Left(self.split_and_insert_left(index, node)),
                    Equal if path_through_self => {
                        SplitResult::Left(self.split_and_insert_right(index, node))
                    }
                    _ => SplitResult::Right(self.split_and_insert_right(index, node)),
                })
            } else {
                self.insert_fitting(index, node);
                InsertResult::Fit
            }
        }
    }

    unsafe fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        self.children_mut().insert(index, node);

        // TODO: this logic is poor
        unsafe { self.set_len(self.len() + 1) };
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let split_index = Self::UNDERFULL_LEN;

        let new_sibling = self.raw_children_mut().split(split_index);
        self.children_mut().insert(index, node);

        let new_self_len = self.children().sum_lens();
        let new_node_len = self.len() - new_self_len + 1;

        debug_assert_eq!(new_node_len, new_sibling.sum_lens());

        unsafe { self.set_len(new_self_len) };
        unsafe { Node::from_children(new_node_len, new_sibling) }
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
    ) -> Node<T, B, C> {
        let split_index = Self::UNDERFULL_LEN + 1;

        let mut new_sibling = self.raw_children_mut().split(split_index);
        new_sibling.as_array_vec().insert(index - split_index, node);

        let new_self_len = self.children().sum_lens();
        let new_node_len = self.len() - new_self_len + 1;

        debug_assert_eq!(new_node_len, new_sibling.sum_lens());

        unsafe { self.set_len(new_self_len) };
        unsafe { Node::from_children(new_node_len, new_sibling) }
    }
}

pub enum InsertResult<T, const B: usize, const C: usize> {
    Fit,
    Split(SplitResult<T, B, C>),
}

pub enum SplitResult<T, const B: usize, const C: usize> {
    Left(Node<T, B, C>),
    Right(Node<T, B, C>),
}
