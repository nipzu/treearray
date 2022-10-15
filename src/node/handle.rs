use core::{
    marker::PhantomData,
    ops::RangeFrom,
    ptr::{self, addr_of_mut},
};

use alloc::boxed::Box;

use crate::{
    node::{InternalNode, NodeBase, NodePtr, RawNodeWithLen, BRANCH_FACTOR},
    ownership,
    utils::ArrayVecMut,
};

impl<'a, T: 'a> LeafRef<'a, T> {
    pub fn value(&self, index: usize) -> Option<&'a T> {
        (index < self.len()).then(|| unsafe { self.value_unchecked(index) })
    }

    pub unsafe fn value_unchecked(&self, index: usize) -> &'a T {
        debug_assert!(self.len() <= NodeBase::<T>::LEAF_CAP);
        debug_assert!(index < self.len());

        // We own a shared reference to this leaf, so there
        // should not be any mutable references which
        // could cause aliasing problems with taking
        // a reference to the whole array.
        unsafe {
            let (_, array_offset) = NodeBase::<T>::leaf_layout();
            &*self
                .node
                .as_ptr()
                .cast::<u8>()
                .add(array_offset)
                .cast::<T>()
                .add(index)
        }
    }
}

impl<'a, T: 'a> InternalRef<'a, T> {
    pub unsafe fn child_containing_index(&self, index: &mut usize) -> NodePtr<T> {
        let mut i = 0;
        for shift in 1..=BRANCH_FACTOR.trailing_zeros() {
            let offset = BRANCH_FACTOR >> shift;
            let v = unsafe { self.node.cast::<InternalNode<T>>().as_mut().lengths[i + offset - 1] };
            if v <= *index {
                *index -= v;
                i += offset;
            }
        }

        debug_assert!(i < self.len_children());
        unsafe { self.node.cast::<InternalNode<T>>().as_mut().children[i].assume_init() }
    }
}

mod height {
    pub struct Zero;
    pub struct One;
    pub struct Positive;
    pub struct TwoOrMore;

    pub unsafe trait Internal {}
    unsafe impl Internal for One {}
    unsafe impl Internal for Positive {}
    unsafe impl Internal for TwoOrMore {}

    pub unsafe trait Height {}
    unsafe impl Height for Zero {}
    unsafe impl<H: Internal> Height for H {}
}

pub struct Node<O, H, T>
where
    H: height::Height,
    O: ownership::Ownership<T>,
{
    node: NodePtr<T>,
    _marker: PhantomData<(H, O)>,
}

pub type InternalRef<'a, T> = Node<ownership::Immut<'a>, height::Positive, T>;
pub type InternalMut<'a, T> = Node<ownership::Mut<'a>, height::Positive, T>;
pub type Internal<T> = Node<ownership::Owned, height::Positive, T>;

pub type LeafRef<'a, T> = Node<ownership::Immut<'a>, height::Zero, T>;
pub type LeafMut<'a, T> = Node<ownership::Mut<'a>, height::Zero, T>;
pub type Leaf<T> = Node<ownership::Owned, height::Zero, T>;

impl<O, H, T> Node<O, H, T>
where
    H: height::Height,
    O: ownership::Ownership<T>,
{
    pub unsafe fn new(ptr: NodePtr<T>) -> Self {
        Self {
            node: ptr,
            _marker: PhantomData,
        }
    }

    pub fn node_ptr(&mut self) -> NodePtr<T> {
        self.node
    }

    fn reborrow(&mut self) -> Node<ownership::Mut, H, T> {
        Node {
            node: self.node,
            _marker: PhantomData,
        }
    }
}

impl<'a, O, H, T: 'a> Node<O, H, T>
where
    H: height::Height,
    O: ownership::Reference<'a, T>,
{
    pub fn into_parent_and_index2(mut self) -> Option<(Node<O, height::Positive, T>, usize)> {
        unsafe {
            let parent = Node::<O, height::Positive, T>::new((*self.node_ptr().as_ptr()).parent?);
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

impl<'a, O, T: 'a> Node<O, height::Zero, T>
where
    O: ownership::Reference<'a, T>,
{
    pub unsafe fn into_parent_and_index3(mut self) -> Option<(Node<O, height::One, T>, usize)> {
        unsafe {
            let parent = Node::<O, height::One, T>::new((*self.node_ptr().as_ptr()).parent?);
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

impl<O, T> Node<O, height::Zero, T>
where
    O: ownership::Ownership<T>,
{
    pub fn len(&self) -> usize {
        unsafe { usize::from(self.node.as_ref().children_len) }
    }
}

impl<'a, T: 'a> LeafMut<'a, T> {
    pub fn values_mut(&mut self) -> ArrayVecMut<T> {
        unsafe {
            let (_, offset) = NodeBase::<T>::leaf_layout();
            let array = self.node.as_ptr().cast::<u8>().add(offset).cast();
            ArrayVecMut::new(
                array,
                addr_of_mut!((*self.node.as_ptr()).children_len).cast(),
                NodeBase::<T>::LEAF_CAP as u16,
            )
        }
    }

    pub unsafe fn into_value_unchecked_mut(self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= NodeBase::<T>::LEAF_CAP);
        debug_assert!(index < len);
        unsafe {
            let (_, offset) = NodeBase::<T>::leaf_layout();
            &mut *self
                .node
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .add(index)
        }
    }

    fn is_full(&self) -> bool {
        self.len() == NodeBase::<T>::LEAF_CAP
    }

    pub fn insert_value(&mut self, index: usize, value: T) -> Option<SplitResult<T>> {
        assert!(index <= self.len());

        if self.is_full() {
            Some(if index <= NodeBase::<T>::LEAF_CAP / 2 {
                SplitResult::Left(self.split_and_insert_left(index, value))
            } else {
                SplitResult::Right(self.split_and_insert_right(index, value))
            })
        } else {
            self.values_mut().insert(index, value);
            None
        }
    }

    fn split_and_insert_left(&mut self, index: usize, value: T) -> RawNodeWithLen<T> {
        let split_index = NodeBase::<T>::LEAF_CAP / 2;
        let new_node = NodeBase::new_leaf();
        let mut new_leaf = unsafe { LeafMut::new(new_node) };
        self.values_mut().split(split_index, new_leaf.values_mut());
        self.values_mut().insert(index, value);
        RawNodeWithLen(new_leaf.values_mut().len(), new_node)
    }

    fn split_and_insert_right(&mut self, index: usize, value: T) -> RawNodeWithLen<T> {
        let split_index = (NodeBase::<T>::LEAF_CAP - 1) / 2 + 1;
        let new_node = NodeBase::new_leaf();
        let mut new_leaf = unsafe { LeafMut::new(new_node) };
        self.values_mut().split(split_index, new_leaf.values_mut());
        new_leaf.values_mut().insert(index - self.len(), value);
        RawNodeWithLen(new_leaf.values_mut().len(), new_node)
    }
}

impl<O, T> Node<O, height::One, T>
where
    O: ownership::Mutable<T>,
{
    pub fn child_mut(&mut self, index: usize) -> LeafMut<T> {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        unsafe { LeafMut::new(ptr.add(index).read().assume_init()) }
    }

    pub fn child_pair_at(&mut self, index: usize) -> [LeafMut<T>; 2] {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        [
            unsafe { LeafMut::new(ptr.add(index).read().assume_init()) },
            unsafe { LeafMut::new(ptr.add(index + 1).read().assume_init()) },
        ]
    }

    pub fn handle_underfull_leaf_child_head(&mut self) {
        let [mut cur, mut next] = self.child_pair_at(0);

        unsafe {
            if next.is_almost_underfull() {
                cur.values_mut().append(next.values_mut());
                self.merge_length_from_next(0);
                Leaf::new(self.children().remove(1)).free();
                self.set_parent_links(1..);
            } else {
                cur.push_back_child(next.pop_front_child());
                self.steal_length_from_next(0, 1);
            }
        }
    }

    pub fn handle_underfull_leaf_child_tail(&mut self, index: &mut usize, child_index: &mut usize) {
        let [mut prev, mut cur] = self.child_pair_at(*index - 1);

        unsafe {
            if prev.is_almost_underfull() {
                let prev_children_len = prev.len();
                prev.values_mut().append(cur.values_mut());
                self.merge_length_from_next(*index - 1);
                Leaf::new(self.children().remove(*index)).free();
                self.set_parent_links(*index..);

                *index -= 1;
                *child_index += prev_children_len;
            } else {
                cur.push_front_child(prev.pop_back_child());
                self.steal_length_from_previous(*index, 1);
                *child_index += 1;
            }
        }
    }
}

impl<O, T> Node<O, height::TwoOrMore, T>
where
    O: ownership::Mutable<T>,
{
    pub unsafe fn new_parent_of_internal(node: NodePtr<T>) -> Self {
        Self {
            node,
            _marker: PhantomData,
        }
    }

    pub fn maybe_handle_underfull_child(&mut self, index: usize) -> bool {
        let is_child_underfull = self.child_mut(index).is_underfull();

        if is_child_underfull {
            if index > 0 {
                self.handle_underfull_internal_child_tail(index);
            } else {
                self.handle_underfull_internal_child_head();
            }
        }

        is_child_underfull
    }

    pub fn child_mut(&mut self, index: usize) -> Node<ownership::Mut, height::Positive, T> {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        Node {
            node: unsafe { ptr.add(index).read().assume_init().cast() },
            _marker: PhantomData,
        }
    }

    pub fn child_pair_at(
        &mut self,
        index: usize,
    ) -> [Node<ownership::Mut, height::Positive, T>; 2] {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        [
            Node {
                node: unsafe { ptr.add(index).read().assume_init().cast() },
                _marker: PhantomData,
            },
            Node {
                node: unsafe { ptr.add(index + 1).read().assume_init().cast() },
                _marker: PhantomData,
            },
        ]
    }

    fn handle_underfull_internal_child_head(&mut self) {
        let [mut cur, mut next] = self.child_pair_at(0);

        if next.is_almost_underfull() {
            unsafe {
                cur.append_children(next);
                self.merge_length_from_next(0);
                Internal::new(self.children().remove(1)).free();
                self.set_parent_links(1..);
            }
        } else {
            unsafe {
                let x = next.pop_front_child();
                let x_len = x.0;
                cur.push_back_child(x);
                self.steal_length_from_next(0, x_len);
            }
        }
    }

    fn handle_underfull_internal_child_tail(&mut self, index: usize) {
        let [mut prev, mut cur] = self.child_pair_at(index - 1);

        if prev.is_almost_underfull() {
            unsafe {
                prev.append_children(cur);
                self.merge_length_from_next(index - 1);
                Internal::new(self.children().remove(index)).free();
                self.set_parent_links(index..);
            }
        } else {
            unsafe {
                let x = prev.pop_back_child();
                let x_len = x.0;
                cur.push_front_child(x);
                self.steal_length_from_previous(index, x_len);
            }
        }
    }
}

impl<T> Leaf<T> {
    pub fn free(self) {
        let (layout, _) = NodeBase::<T>::leaf_layout();
        unsafe { alloc::alloc::dealloc(self.node.cast().as_ptr(), layout) }
    }
}

impl<T> Internal<T> {
    pub fn free(self) {
        // debug_assert_eq!(self.node.base.children_len, 0);
        // debug_assert_eq!(self.node.len(), 0);
        unsafe { Box::from_raw(self.node.cast::<InternalNode<T>>().as_ptr()) };
    }
}

impl<'a, T: 'a> LeafMut<'a, T> {
    const UNDERFULL_LEN: usize = (NodeBase::<T>::LEAF_CAP - 1) / 2;
    pub fn remove_child(&mut self, index: usize) -> T {
        self.values_mut().remove(index)
    }
    fn push_front_child(&mut self, child: T) {
        self.values_mut().insert(0, child);
    }
    fn push_back_child(&mut self, child: T) {
        self.values_mut().insert(self.len(), child);
    }
    fn pop_front_child(&mut self) -> T {
        self.remove_child(0)
    }
    fn pop_back_child(&mut self) -> T {
        self.remove_child(self.len() - 1)
    }
    pub fn is_underfull(&self) -> bool {
        self.len() <= Self::UNDERFULL_LEN
    }
    fn is_almost_underfull(&self) -> bool {
        self.len() <= Self::UNDERFULL_LEN + 1
    }
}

impl<O, H, T> Node<O, H, T>
where
    H: height::Internal,
    O: ownership::Ownership<T>,
{
    pub const UNDERFULL_LEN: usize = (BRANCH_FACTOR - 1) / 2;

    fn node(&self) -> &InternalNode<T> {
        unsafe { self.node.cast().as_ref() }
    }

    pub fn len_children(&self) -> usize {
        usize::from(self.node().base.children_len)
    }

    pub fn is_singleton(&self) -> bool {
        self.len_children() == 1
    }

    fn is_full(&self) -> bool {
        self.len_children() == BRANCH_FACTOR
    }

    pub fn is_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN
    }

    pub fn internal_ptr(&mut self) -> *mut InternalNode<T> {
        self.node.cast().as_ptr()
    }

    pub fn len(&mut self) -> usize {
        self.node().lengths[BRANCH_FACTOR - 1]
    }

    pub fn sum_lens_below(&self, mut index: usize) -> usize {
        let mut sum = 0;
        assert!(index <= self.node().lengths.len());
        while index != 0 {
            sum += self.node().lengths[index - 1];
            index &= index - 1;
        }
        sum
    }
}

impl<O, H, T> Node<O, H, T>
where
    H: height::Internal,
    O: ownership::Mutable<T>,
{
    unsafe fn push_front_child(&mut self, child: RawNodeWithLen<T>) {
        unsafe {
            self.push_front_length(child.0);
            self.children().insert(0, child.1);
            self.set_parent_links(0..);
        }
    }
    pub unsafe fn push_back_child(&mut self, child: RawNodeWithLen<T>) {
        unsafe { self.push_back_length(child.0) };
        self.children().insert(self.len_children(), child.1);
        self.set_parent_links(self.len_children() - 1..);
    }
    unsafe fn pop_front_child(&mut self) -> RawNodeWithLen<T> {
        let node_len = unsafe { self.pop_front_length() };
        let node = self.children().remove(0);
        self.set_parent_links(0..);
        RawNodeWithLen(node_len, node)
    }
    unsafe fn pop_back_child(&mut self) -> RawNodeWithLen<T> {
        let last_len = unsafe { self.pop_back_length() };
        let last = self.children().pop_back();
        RawNodeWithLen(last_len, last)
    }
    fn is_almost_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN + 1
    }
    unsafe fn append_children(&mut self, mut other: Self) {
        let self_old_len = self.len_children();
        unsafe { self.append_lengths(other.reborrow()) };
        self.children().append(other.children());
        self.set_parent_links(self_old_len..);
    }

    pub fn internal_mut(&mut self) -> &mut InternalNode<T> {
        unsafe { self.node.cast().as_mut() }
    }

    fn lengths_mut(&mut self) -> &mut [usize; BRANCH_FACTOR] {
        &mut self.internal_mut().lengths
    }

    unsafe fn steal_length_from_next(&mut self, index: usize, amount: usize) {
        unsafe {
            self.add_length_wrapping(index + 1, amount.wrapping_neg());
            self.add_length_wrapping(index, amount);
        }
    }

    unsafe fn steal_length_from_previous(&mut self, index: usize, amount: usize) {
        unsafe {
            self.add_length_wrapping(index - 1, amount.wrapping_neg());
            self.add_length_wrapping(index, amount);
        }
    }

    unsafe fn init(&mut self) {
        for index in 0..self.lengths_mut().len() {
            let j = index | (index + 1);
            if j < self.lengths_mut().len() {
                self.lengths_mut()[j] += self.lengths_mut()[index];
            }
        }
    }

    unsafe fn fini(&mut self) {
        for index in (0..self.lengths_mut().len()).rev() {
            let j = index | (index + 1);
            if j < self.lengths_mut().len() {
                self.lengths_mut()[j] -= self.lengths_mut()[index];
            }
        }
    }

    unsafe fn append_lengths<'b>(&'b mut self, mut other: Node<ownership::Mut<'b>, H, T>) {
        unsafe { self.fini() };
        unsafe { other.fini() };
        let len_children = self.len_children();
        for i in 0..other.len_children() {
            self.lengths_mut()[len_children + i] = other.lengths_mut()[i];
        }
        unsafe { self.init() };
    }

    unsafe fn merge_length_from_next(&mut self, index: usize) {
        unsafe {
            self.fini();
            let lens_ptr = self.lengths_mut().as_mut_ptr();
            let next_len = lens_ptr.add(index + 1).read();
            (*lens_ptr.add(index)) += next_len;
            ptr::copy(
                lens_ptr.add(index + 2),
                lens_ptr.add(index + 1),
                BRANCH_FACTOR - index - 2,
            );
            self.lengths_mut()[BRANCH_FACTOR - 1] = 0;
            self.init();
        }
    }

    unsafe fn insert_length(&mut self, index: usize, len: usize) {
        unsafe { self.fini() };
        for i in (index..self.len_children()).rev() {
            self.lengths_mut()[i + 1] = self.lengths_mut()[i];
        }
        self.lengths_mut()[index] = len;

        unsafe { self.init() };
    }

    unsafe fn split_lengths<'b>(
        &'b mut self,
        index: usize,
        mut other: Node<ownership::Mut<'b>, H, T>,
    ) {
        unsafe { self.fini() };
        for i in index..self.len_children() {
            other.lengths_mut()[i - index] = self.lengths_mut()[i];
            self.lengths_mut()[i] = 0;
        }
        unsafe { other.init() };
        unsafe { self.init() };
    }

    unsafe fn pop_front_length(&mut self) -> usize {
        unsafe {
            self.fini();
            let lens_ptr = self.lengths_mut().as_mut_ptr();
            let first_len = lens_ptr.read();
            for i in 1..self.len_children() {
                self.lengths_mut()[i - 1] = self.lengths_mut()[i];
            }
            let len_children = self.len_children();
            self.lengths_mut()[len_children - 1] = 0;
            self.init();
            first_len
        }
    }

    unsafe fn pop_back_length(&mut self) -> usize {
        unsafe { self.fini() };
        let len = self.len_children();
        let ret = core::mem::take(&mut self.lengths_mut()[len - 1]);
        unsafe { self.init() };
        ret
    }

    unsafe fn push_back_length(&mut self, len: usize) {
        unsafe {
            self.fini();
            let children_len = self.len_children();
            self.lengths_mut()[children_len] = len;
            self.init();
        }
    }

    unsafe fn push_front_length(&mut self, len: usize) {
        unsafe { self.fini() };
        for i in (0..self.len_children()).rev() {
            self.lengths_mut()[i + 1] = self.lengths_mut()[i];
        }
        self.lengths_mut()[0] = len;
        unsafe { self.init() };
    }

    pub unsafe fn add_length_wrapping(&mut self, mut index: usize, amount: usize) {
        while let Some(v) = self.lengths_mut().get_mut(index) {
            *v = v.wrapping_add(amount);
            index |= index + 1;
        }
    }

    fn node_mut(&mut self) -> &mut InternalNode<T> {
        unsafe { self.node.cast().as_mut() }
    }

    pub fn children(&mut self) -> ArrayVecMut<NodePtr<T>> {
        unsafe {
            ArrayVecMut::new(
                addr_of_mut!((*self.internal_ptr()).children).cast(),
                addr_of_mut!(self.node_mut().base.children_len).cast(),
                BRANCH_FACTOR as u16,
            )
        }
    }

    pub unsafe fn into_parent_and_index<'a>(
        mut self,
    ) -> Option<(Node<O, height::TwoOrMore, T>, usize)>
    where
        T: 'a,
        O: ownership::Reference<'a, T>,
    {
        unsafe {
            let parent = Node::new_parent_of_internal(self.node_mut().base.parent?);
            Some((
                parent,
                self.node_mut().base.parent_index.assume_init().into(),
            ))
        }
    }

    pub unsafe fn into_child_containing_index(mut self, index: &mut usize) -> NodePtr<T> {
        let mut i = 0;
        for shift in 1..=BRANCH_FACTOR.trailing_zeros() {
            let offset = BRANCH_FACTOR >> shift;
            let v = self.lengths_mut()[i + offset - 1];
            if v <= *index {
                *index -= v;
                i += offset;
            }
        }

        debug_assert!(i < self.len_children());
        unsafe { self.internal_mut().children[i].assume_init() }
    }

    pub unsafe fn insert_split_of_child(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T>,
    ) -> Option<RawNodeWithLen<T>> {
        unsafe {
            self.add_length_wrapping(index, node.0.wrapping_neg());
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

    unsafe fn insert_fitting(&mut self, index: usize, node: RawNodeWithLen<T>) {
        debug_assert!(!self.is_full());
        unsafe {
            self.insert_length(index, node.0);
        }
        self.children().insert(index, node.1);
        self.set_parent_links(index..);
    }

    unsafe fn split_and_insert_left(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T>,
    ) -> RawNodeWithLen<T> {
        let split_index = Self::UNDERFULL_LEN;

        let new_sibling_node = InternalNode::<T>::new(self.node().base.height);
        let mut new_sibling = unsafe { Node::<ownership::Mut, H, T>::new(new_sibling_node) };

        unsafe {
            self.split_lengths(split_index, new_sibling.reborrow());
            self.children().split(split_index, new_sibling.children());
            self.insert_fitting(index, node);
        };

        new_sibling.set_parent_links(0..);
        RawNodeWithLen(new_sibling.len(), new_sibling_node)
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T>,
    ) -> RawNodeWithLen<T> {
        let split_index = Self::UNDERFULL_LEN + 1;

        let new_sibling_node = InternalNode::<T>::new(self.node().base.height);

        let mut new_sibling = unsafe { Node::<ownership::Mut, H, T>::new(new_sibling_node) };

        unsafe {
            self.split_lengths(split_index, new_sibling.reborrow());
            self.children().split(split_index, new_sibling.children());
            new_sibling.insert_fitting(index - split_index, node);
        }

        new_sibling.set_parent_links(0..);
        RawNodeWithLen(new_sibling.len(), new_sibling_node)
    }

    fn set_parent_links(&mut self, range: RangeFrom<usize>) {
        for (i, n) in self.children()[range.clone()].iter_mut().enumerate() {
            unsafe {
                (*n.as_ptr()).parent = Some(self.node_ptr());
                (*n.as_ptr()).parent_index.write((i + range.start) as u8);
            }
        }
    }
}

pub enum SplitResult<T> {
    Left(RawNodeWithLen<T>),
    Right(RawNodeWithLen<T>),
}
