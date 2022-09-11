use core::{
    hint::unreachable_unchecked,
    marker::PhantomData,
    ops::Range,
    ptr::{addr_of_mut, NonNull},
};

use alloc::boxed::Box;

use crate::{
    node::{InternalNode, LeafNode, Node, NodeBase},
    utils::ArrayVecMut,
};

pub struct Leaf<'a, T, const B: usize, const C: usize> {
    node: &'a LeafNode<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Leaf<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new(node: &'a LeafNode<T, B, C>) -> Self {
        let this = Self { node };
        debug_assert!(this.len() <= C);
        this
    }

    pub fn len(&self) -> usize {
        unsafe { usize::from(self.node.base.children_len.assume_init()) }
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
                .values
                .as_ref()
                .get_unchecked(index)
                .assume_init_ref()
        }
    }
}

pub struct Internal<'a, T, const B: usize, const C: usize> {
    node: &'a InternalNode<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Internal<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub const unsafe fn new(node: &'a InternalNode<T, B, C>) -> Self {
        Self { node }
    }

    pub unsafe fn child_containing_index(&self, index: &mut usize) -> &'a Node<T, B, C> {
        for child in &self.node.children {
            let child = unsafe { child.assume_init_ref() };
            let len = child.len();
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return child,
            }
        }

        unsafe { unreachable_unchecked() };
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

use height::ExactInternal;

pub struct OwnedNode<H, T, const B: usize, const C: usize> {
    node: Node<T, B, C>,
    _height: H,
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Positive, T, B, C> {
    pub const unsafe fn new_internal(node: Node<T, B, C>) -> Self {
        Self {
            node,
            _height: height::Positive,
        }
    }
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Zero, T, B, C> {
    pub fn as_mut(&mut self) -> LeafMut<T, B, C> {
        LeafMut {
            node: self.node.ptr.cast(),
            lifetime: PhantomData,
        }
    }
}

impl<T, const B: usize, const C: usize> OwnedNode<height::Zero, T, B, C> {
    pub const unsafe fn new_leaf(node: Node<T, B, C>) -> Self {
        Self {
            node,
            _height: height::Zero,
        }
    }

    pub fn new_empty_leaf() -> Self {
        let node = Node::empty_leaf();
        Self {
            node,
            _height: height::Zero,
        }
    }
}

pub struct LeafMut<'a, T, const B: usize, const C: usize> {
    node: NonNull<LeafNode<T, B, C>>,
    lifetime: PhantomData<&'a mut Node<T, B, C>>,
}

pub struct InternalMut<'a, H, T, const B: usize, const C: usize> {
    pub node: NonNull<InternalNode<T, B, C>>,
    height: H,
    lifetime: PhantomData<&'a mut Node<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> LeafMut<'a, T, B, C> {
    pub fn len(&self) -> usize {
        unsafe { (*self.node.as_ptr()).base.children_len.assume_init().into() }
    }

    pub const fn node_ptr(&self) -> NonNull<LeafNode<T, B, C>> {
        self.node
    }

    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new_leaf(node: NonNull<LeafNode<T, B, C>>) -> LeafMut<'a, T, B, C> {
        // debug_assert!(unsafe { (*node).len() <= C });

        Self {
            node,
            lifetime: PhantomData,
        }
    }

    pub fn values_mut(&mut self) -> ArrayVecMut<T, C> {
        unsafe {
            ArrayVecMut::new(
                addr_of_mut!((*self.node.as_ptr()).values),
                addr_of_mut!((*self.node.as_ptr()).base.children_len).cast(),
            )
        }
    }

    pub unsafe fn into_value_unchecked_mut(self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= C);
        debug_assert!(index < len);
        unsafe {
            (*self.node.as_ptr())
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
        let mut new_node = OwnedNode::new_empty_leaf();
        let mut new_leaf = new_node.as_mut();
        self.values_mut().split(split_index, new_leaf.values_mut());
        self.values_mut().insert(index, value);
        new_node.node.length = new_leaf.values_mut().len();
        new_node.node
    }

    fn split_and_insert_right(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let split_index = (C - 1) / 2 + 1;
        let mut new_node = OwnedNode::new_empty_leaf();
        let mut new_leaf = new_node.as_mut();
        self.values_mut().split(split_index, new_leaf.values_mut());
        new_leaf.values_mut().insert(index - self.len(), value);
        new_node.node.length = new_leaf.values_mut().len();
        new_node.node
    }
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, height::One, T, B, C> {
    pub unsafe fn new_parent_of_leaf(node: NonNull<InternalNode<T, B, C>>) -> Self {
        Self {
            node,
            height: height::One,
            lifetime: PhantomData,
        }
    }

    pub fn child_mut(&mut self, index: usize) -> LeafMut<T, B, C> {
        let ptr = unsafe { (*self.node.as_ptr()).children.as_mut_ptr() };
        LeafMut {
            node: unsafe { ptr.add(index).read().assume_init().ptr.cast() },
            lifetime: PhantomData,
        }
    }

    pub fn child_pair_at(&mut self, index: usize) -> [LeafMut<T, B, C>; 2] {
        let ptr = unsafe { (*self.node.as_ptr()).children.as_mut_ptr() };
        [
            LeafMut {
                node: unsafe { ptr.add(index).read().assume_init().ptr.cast() },
                lifetime: PhantomData,
            },
            LeafMut {
                node: unsafe { ptr.add(index + 1).read().assume_init().ptr.cast() },
                lifetime: PhantomData,
            },
        ]
    }

    pub fn handle_underfull_leaf_child_head(&mut self) {
        let [mut cur, mut next] = self.child_pair_at(0);

        if next.is_almost_underfull() {
            cur.append_children(next);
            unsafe {
                OwnedNode::new_internal(self.as_array_vec().remove(1)).free();
            }
        } else {
            cur.push_back_child(next.pop_front_child());
        }
    }

    pub fn handle_underfull_leaf_child_tail(&mut self, index: &mut usize, child_index: &mut usize) {
        let [mut prev, mut cur] = self.child_pair_at(*index - 1);

        if prev.is_almost_underfull() {
            let prev_len = prev.len_children();
            prev.append_children(cur);
            unsafe {
                OwnedNode::new_internal(self.as_array_vec().remove(*index)).free();
            }
            *index -= 1;
            *child_index += prev_len;
        } else {
            cur.push_front_child(prev.pop_back_child());
            *child_index += 1;
        }
    }
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, height::TwoOrMore, T, B, C> {
    pub unsafe fn new_parent_of_internal(node: NonNull<InternalNode<T, B, C>>) -> Self {
        Self {
            node,
            height: height::TwoOrMore,
            lifetime: PhantomData,
        }
    }

    pub fn with_brand<F, R>(&mut self, f: F) -> R
    where
        F: for<'new_id> FnOnce(InternalMut<height::BrandedTwoOrMore<'new_id>, T, B, C>) -> R,
    {
        let parent = InternalMut {
            node: self.node,
            height: height::BrandedTwoOrMore::new(),
            lifetime: PhantomData,
        };

        f(parent)
    }
}

impl<'a, 'id, T, const B: usize, const C: usize>
    InternalMut<'a, height::BrandedTwoOrMore<'id>, T, B, C>
{
    pub fn set_child_parent_cache(&mut self, index: usize) {
        let self_node = self.node;
        self.child_mut(index)
            .set_parent_node_and_index(self_node, index);
    }

    pub fn child_mut(
        &mut self,
        index: usize,
    ) -> InternalMut<height::ChildOf<height::BrandedTwoOrMore<'id>>, T, B, C> {
        let height = unsafe { self.height.make_child_height() };
        let ptr = unsafe { (*self.node.as_ptr()).children.as_mut_ptr() };
        InternalMut {
            node: unsafe { ptr.add(index).read().assume_init().ptr.cast() },
            height,
            lifetime: PhantomData,
        }
    }

    pub fn child_pair_at(
        &mut self,
        index: usize,
    ) -> [InternalMut<height::ChildOf<height::BrandedTwoOrMore<'id>>, T, B, C>; 2] {
        let (h1, h2) = unsafe {
            (
                self.height.make_child_height(),
                self.height.make_child_height(),
            )
        };
        let ptr = unsafe { (*self.node.as_ptr()).children.as_mut_ptr() };
        [
            InternalMut {
                node: unsafe { ptr.add(index).read().assume_init().ptr.cast() },
                height: h1,
                lifetime: PhantomData,
            },
            InternalMut {
                node: unsafe { ptr.add(index + 1).read().assume_init().ptr.cast() },
                height: h2,
                lifetime: PhantomData,
            },
        ]
    }

    pub fn handle_underfull_internal_child_head(&mut self) {
        let [mut cur, mut next] = self.child_pair_at(0);

        if next.is_almost_underfull() {
            cur.append_children(next);
            unsafe {
                OwnedNode::new_internal(self.as_array_vec().remove(1)).free();
            }
        } else {
            cur.push_back_child(next.pop_front_child());
        }
    }

    pub fn handle_underfull_internal_child_tail(
        &mut self,
        index: &mut usize,
        child_index: &mut usize,
    ) {
        let [mut prev, mut cur] = self.child_pair_at(*index - 1);

        if prev.is_almost_underfull() {
            let prev_len = prev.len_children();
            prev.append_children(cur);
            unsafe {
                OwnedNode::new_internal(self.as_array_vec().remove(*index)).free();
            }
            *index -= 1;
            *child_index += prev_len;
        } else {
            cur.push_front_child(prev.pop_back_child());
            *child_index += 1;
        }
    }
}

pub trait FreeableNode {
    fn free(self);
}

impl<T, const B: usize, const C: usize> FreeableNode for OwnedNode<height::Zero, T, B, C> {
    fn free(self) {
        debug_assert_eq!(self.node.len(), 0);
        unsafe { Box::from_raw(self.node.ptr.as_ptr().cast::<LeafNode<T, B, C>>()) };
    }
}

impl<H, T, const B: usize, const C: usize> FreeableNode for OwnedNode<H, T, B, C>
where
    H: height::Internal + Copy,
{
    fn free(self) {
        // debug_assert_eq!(unsafe { (*self.node.ptr.children.as_ptr()).base.children_len.assume_init() }, 0);
        debug_assert_eq!(self.node.len(), 0);
        unsafe { Box::from_raw(self.node.ptr.as_ptr().cast::<InternalNode<T, B, C>>()) };
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

unsafe impl<'a, T, const B: usize, const C: usize> ExactHeightNode for LeafMut<'a, T, B, C> {
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

unsafe impl<'a, H, T, const B: usize, const C: usize> ExactHeightNode
    for InternalMut<'a, H, T, B, C>
where
    H: height::ExactInternal,
{
    type Child = OwnedNode<H::ChildHeight, T, B, C>;
    const UNDERFULL_LEN: usize = (B - 1) / 2;
    fn len_children(&self) -> usize {
        self.len_children()
    }
    fn insert_child(&mut self, index: usize, child: Self::Child) {
        // unsafe { self.set_len(self.len() + child.node.len()) };
        self.as_array_vec().insert(index, child.node);
    }
    fn remove_child(&mut self, index: usize) -> Self::Child {
        let node = self.as_array_vec().remove(index);
        // unsafe { self.set_len(self.len() - node.len()) };
        OwnedNode {
            node,
            _height: unsafe { self.height.make_child_height() },
        }
    }
    fn append_children(&mut self, mut other: Self) {
        self.as_array_vec().append(other.as_array_vec());

        // unsafe { self.set_len(self.len() + other.len()) };
        // unsafe { other.set_len(0) };
    }
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, height::Positive, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(node: NonNull<NodeBase<T, B, C>>) -> Self {
        Self {
            node: node.cast::<InternalNode<T, B, C>>(),
            height: height::Positive,
            lifetime: PhantomData,
        }
    }
}

impl<'a, H, T, const B: usize, const C: usize> InternalMut<'a, H, T, B, C>
where
    H: height::Internal,
{
    pub const UNDERFULL_LEN: usize = (B - 1) / 2;

    pub fn len_children(&self) -> usize {
        unsafe { (*self.node.as_ptr()).base.children_len.assume_init().into() }
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

    pub fn set_parent_node_and_index(
        &mut self,
        parent: NonNull<InternalNode<T, B, C>>,
        index: usize,
    ) {
        self.set_parent_index(index);
        unsafe { (*self.node.as_ptr()).base.parent = Some(parent) }
    }

    pub fn set_parent_index(&mut self, index: usize) {
        unsafe {
            (*self.node.as_ptr()).base.parent_index.write(index as u16);
        }
    }

    pub fn as_array_vec(&mut self) -> ArrayVecMut<Node<T, B, C>, B> {
        unsafe {
            ArrayVecMut::new(
                addr_of_mut!((*self.node.as_ptr()).children),
                addr_of_mut!((*self.node.as_ptr()).base.children_len).cast(),
            )
        }
    }

    pub unsafe fn into_parent_and_index(
        self,
    ) -> Option<(InternalMut<'a, height::TwoOrMore, T, B, C>, u16)> {
        unsafe {
            let parent = InternalMut::new_parent_of_internal((*self.node.as_ptr()).base.parent?);
            Some((
                parent,
                (*self.node.as_ptr()).base.parent_index.assume_init(),
            ))
        }
    }

    pub fn into_internal(self) -> InternalMut<'a, height::Positive, T, B, C> {
        InternalMut {
            node: self.node,
            height: height::Positive,
            lifetime: self.lifetime,
        }
    }

    pub unsafe fn into_child_containing_index(self, index: &mut usize) -> &'a mut Node<T, B, C> {
        // debug_assert!(*index < self.len());
        unsafe {
            for child in &mut (*self.node.as_ptr()).children {
                let len = child.assume_init_mut().len();
                match index.checked_sub(len) {
                    Some(r) => *index = r,
                    None => return child.assume_init_mut(),
                }
            }
        }

        debug_assert!(false);
        unsafe { unreachable_unchecked() };
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
        self.as_array_vec().insert(index, node);
        
        let self_children_len = self.as_array_vec().len();
        self.set_parent_links(index..self_children_len);
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let split_index = Self::UNDERFULL_LEN;

        let mut new_sibling_node = Node::from_child_array([]);
        let mut new_sibling = unsafe { InternalMut::new(new_sibling_node.ptr.cast()) };

        self.as_array_vec()
            .split(split_index, new_sibling.as_array_vec());
        unsafe { self.insert_fitting(index, node) };

        let new_children_len = new_sibling.as_array_vec().len();
        new_sibling.set_parent_links(0..new_children_len);

        new_sibling_node.length = unsafe { new_sibling.node.as_ref().sum_lens() };
        new_sibling_node
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
    ) -> Node<T, B, C> {
        let split_index = Self::UNDERFULL_LEN + 1;

        let mut new_sibling_node = Node::from_child_array([]);
        let mut new_sibling = unsafe { InternalMut::new(new_sibling_node.ptr.cast()) };

        self.as_array_vec()
            .split(split_index, new_sibling.as_array_vec());
        new_sibling.as_array_vec().insert(index - split_index, node);

        let new_children_len = new_sibling.as_array_vec().len();
        new_sibling.set_parent_links(0..new_children_len);

        new_sibling_node.length = unsafe { new_sibling.node.as_ref().sum_lens() };
        new_sibling_node
    }

    fn set_parent_links(&mut self, range: Range<usize>) {
        for (i, n) in self.as_array_vec()[range.clone()].iter_mut().enumerate() {
            unsafe {
                (*n.ptr.as_ptr()).parent = Some(self.node);
                (*n.ptr.as_ptr())
                    .parent_index
                    .write((i + range.start) as u16);
            }
        }
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
