use core::{
    mem::MaybeUninit,
    ops::RangeFrom,
    ptr::{self, addr_of_mut},
};

use alloc::boxed::Box;

use crate::{
    node::{InternalNode, LeafNode, NodePtr, RawNodeWithLen},
    utils::ArrayVecMut,
};

impl<'a, T: 'a, const B: usize, const C: usize> LeafRef<'a, T, B, C> {
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

impl<'a, T: 'a, const B: usize, const C: usize> InternalRef<'a, T, B, C> {
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

pub type InternalRef<'a, T, const B: usize, const C: usize> =
    Node<ownership::Immut<'a>, height::Positive, T, B, C>;
pub type InternalMut<'a, T, const B: usize, const C: usize> =
    Node<ownership::Mut<'a>, height::Positive, T, B, C>;
pub type Internal<T, const B: usize, const C: usize> =
    Node<ownership::Owned, height::Positive, T, B, C>;

pub type LeafRef<'a, T, const B: usize, const C: usize> =
    Node<ownership::Immut<'a>, height::Zero, T, B, C>;
pub type LeafMut<'a, T, const B: usize, const C: usize> =
    Node<ownership::Mut<'a>, height::Zero, T, B, C>;
pub type Leaf<T, const B: usize, const C: usize> = Node<ownership::Owned, height::Zero, T, B, C>;

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Height,
    O: ownership::NodePtrType<H::NodeType<T, B, C>>,
{
    pub unsafe fn new(ptr: NodePtr<T, B, C>) -> Self {
        Self {
            node: unsafe { O::from_raw(ptr.cast()) },
        }
    }

    pub fn node_ptr(&mut self) -> NodePtr<T, B, C> {
        O::as_raw(&mut self.node).cast()
    }
}

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Height,
    O: ownership::NodePtrType<H::NodeType<T, B, C>>,
{
    fn reborrow<'b>(&'b mut self) -> Node<ownership::Mut<'b>, H, T, B, C> {
        Node {
            node: O::as_raw(&mut self.node),
        }
    }
}

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    O: ownership::NodePtrType<H::NodeType<T, B, C>> + ownership::NodePtrType<InternalNode<T, B, C>>,
    H: height::Height,
{
    pub unsafe fn into_parent_and_index2(
        mut self,
    ) -> Option<(Node<O, height::Positive, T, B, C>, usize)>
    where
        O: ownership::Reference,
    {
        unsafe {
            let parent =
                Node::<O, height::Positive, T, B, C>::new((*self.node_ptr().as_ptr()).parent?);
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
    O: ownership::NodePtrType<LeafNode<T, B, C>> + ownership::NodePtrType<InternalNode<T, B, C>>,
{
    pub unsafe fn into_parent_and_index3(mut self) -> Option<(Node<O, height::One, T, B, C>, usize)>
    where
        O: ownership::Reference,
    {
        unsafe {
            let parent = Node::<O, height::One, T, B, C>::new((*self.node_ptr().as_ptr()).parent?);
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
        let new_node = LeafNode::<T, B, C>::new().cast();
        let mut new_leaf = unsafe { LeafMut::new(new_node) };
        self.values_mut().split(split_index, new_leaf.values_mut());
        self.values_mut().insert(index, value);
        RawNodeWithLen(new_leaf.values_mut().len(), new_node)
    }

    fn split_and_insert_right(&mut self, index: usize, value: T) -> RawNodeWithLen<T, B, C> {
        let split_index = (C - 1) / 2 + 1;
        let new_node = LeafNode::<T, B, C>::new().cast();
        let mut new_leaf = unsafe { LeafMut::new(new_node) };
        self.values_mut().split(split_index, new_leaf.values_mut());
        new_leaf.values_mut().insert(index - self.len(), value);
        RawNodeWithLen(new_leaf.values_mut().len(), new_node)
    }
}

impl<O, T, const B: usize, const C: usize> Node<O, height::One, T, B, C>
where
    O: ownership::NodePtrType<InternalNode<T, B, C>> + ownership::Mutable,
{
    pub fn child_mut(&mut self, index: usize) -> LeafMut<T, B, C> {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        unsafe { LeafMut::new(ptr.add(index).read().assume_init()) }
    }

    pub fn child_pair_at(&mut self, index: usize) -> [LeafMut<T, B, C>; 2] {
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
    O: ownership::NodePtrType<InternalNode<T, B, C>> + ownership::Mutable,
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

impl<T, const B: usize, const C: usize> Leaf<T, B, C> {
    pub fn free(self) {
        debug_assert_eq!(self.node.base.children_len, 0);
        unsafe { Box::from_raw(Box::into_raw(self.node).cast::<LeafNode<T, B, C>>()) };
    }
}

impl<T, const B: usize, const C: usize> Internal<T, B, C> {
    pub fn free(self) {
        debug_assert_eq!(self.node.base.children_len, 0);
        // debug_assert_eq!(self.node.len(), 0);
        unsafe { Box::from_raw(Box::into_raw(self.node).cast::<InternalNode<T, B, C>>()) };
    }
}

impl<'a, T: 'a, const B: usize, const C: usize> LeafMut<'a, T, B, C> {
    const UNDERFULL_LEN: usize = (C - 1) / 2;
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

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Internal,
    O: ownership::NodePtrType<InternalNode<T, B, C>> + ownership::Mutable,
{
    unsafe fn push_front_child(&mut self, child: RawNodeWithLen<T, B, C>) {
        unsafe {
            self.push_front_length(child.0);
            self.children().insert(0, child.1);
            self.set_parent_links(0..);
        }
    }
    unsafe fn push_back_child(&mut self, child: RawNodeWithLen<T, B, C>) {
        unsafe { self.push_back_length(child.0) };
        self.children().insert(self.len_children(), child.1);
        self.set_parent_links(self.len_children() - 1..);
    }
    unsafe fn pop_front_child(&mut self) -> RawNodeWithLen<T, B, C> {
        let node = self.children().remove(0);
        let node_len = unsafe { self.pop_front_length() };
        self.set_parent_links(0..);
        RawNodeWithLen(node_len, node)
    }
    unsafe fn pop_back_child(&mut self) -> RawNodeWithLen<T, B, C> {
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
}

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Internal,
    O: ownership::NodePtrType<InternalNode<T, B, C>> + ownership::Mutable,
{
    pub fn internal_mut(&mut self) -> &mut InternalNode<T, B, C> {
        unsafe { O::as_raw(&mut self.node).cast().as_mut() }
    }

    unsafe fn steal_length_from_next(&mut self, index: usize, amount: usize) {
        unsafe {
            *self.internal_mut().lengths[index + 1].assume_init_mut() -= amount;
            *self.internal_mut().lengths[index].assume_init_mut() += amount;
        }
    }

    unsafe fn steal_length_from_previous(&mut self, index: usize, amount: usize) {
        unsafe {
            *self.internal_mut().lengths[index - 1].assume_init_mut() -= amount;
            *self.internal_mut().lengths[index].assume_init_mut() += amount;
        }
    }

    unsafe fn append_lengths<'b>(&'b mut self, mut other: Node<ownership::Mut<'b>, H, T, B, C>) {
        unsafe {
            let lens_src = other.internal_mut().lengths.as_ptr();
            let lens_dst = self
                .internal_mut()
                .lengths
                .as_mut_ptr()
                .add(self.children().len());
            ptr::copy_nonoverlapping(lens_src, lens_dst, other.children().len());
        }
    }

    unsafe fn merge_length_from_next(&mut self, index: usize) {
        unsafe {
            let lens_ptr = self.internal_mut().lengths.as_mut_ptr();
            let next_len = lens_ptr.add(index + 1).read().assume_init();
            *(*lens_ptr.add(index)).assume_init_mut() += next_len;
            ptr::copy(
                lens_ptr.add(index + 2),
                lens_ptr.add(index + 1),
                B - index - 2,
            );
        }
    }

    unsafe fn insert_length(&mut self, index: usize, len: usize) {
        unsafe {
            let lens_ptr = self.internal_mut().lengths.as_mut_ptr();
            ptr::copy(
                lens_ptr.add(index),
                lens_ptr.add(index + 1),
                self.len_children() - index,
            );
            ptr::write(lens_ptr.add(index), MaybeUninit::new(len));
        }
    }

    unsafe fn split_lengths<'b>(
        &'b mut self,
        index: usize,
        mut other: Node<ownership::Mut<'b>, H, T, B, C>,
    ) {
        unsafe {
            let tail_len = self.len_children() - index;
            let lens_ptr = self.internal_mut().lengths.as_ptr();
            ptr::copy_nonoverlapping(
                lens_ptr.add(index),
                other.internal_mut().lengths.as_mut_ptr(),
                tail_len,
            );
        }
    }

    unsafe fn pop_front_length(&mut self) -> usize {
        unsafe {
            let lens_ptr = self.internal_mut().lengths.as_mut_ptr();
            let first_len = lens_ptr.read().assume_init();
            ptr::copy(lens_ptr.add(1), lens_ptr, B - 1);
            first_len
        }
    }

    unsafe fn pop_back_length(&mut self) -> usize {
        unsafe {
            let len = self.len_children();
            self.internal_mut().lengths[len - 1].assume_init()
        }
    }

    unsafe fn push_back_length(&mut self, len: usize) {
        unsafe {
            let children_len = self.len_children();
            let lens_ptr = self.internal_mut().lengths.as_mut_ptr();
            ptr::write(lens_ptr.add(children_len), MaybeUninit::new(len));
        }
    }

    unsafe fn push_front_length(&mut self, len: usize) {
        unsafe {
            let lens_ptr = self.internal_mut().lengths.as_mut_ptr();
            ptr::copy(lens_ptr, lens_ptr.add(1), self.len_children());
            ptr::write(lens_ptr, MaybeUninit::new(len));
        }
    }

    pub unsafe fn update_length<F: Fn(usize) -> usize>(&mut self, index: usize, f: F) {
        let length = unsafe { self.internal_mut().lengths[index as usize].assume_init_mut() };
        *length = f(*length);
    }
}
impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Internal,
    O: ownership::NodePtrType<InternalNode<T, B, C>>,
{
    pub const UNDERFULL_LEN: usize = (B - 1) / 2;
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
    pub fn internal_ptr(&mut self) -> *mut InternalNode<T, B, C> {
        O::as_raw(&mut self.node).cast().as_ptr()
    }
}

impl<O, H, T, const B: usize, const C: usize> Node<O, H, T, B, C>
where
    H: height::Internal,
    O: ownership::NodePtrType<InternalNode<T, B, C>> + ownership::Mutable,
{
    pub fn children(&mut self) -> ArrayVecMut<NodePtr<T, B, C>, B> {
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
            for (i, len) in &mut self.internal_mut().lengths.iter().enumerate() {
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
            self.update_length(index, |l| l - node.0);
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
        unsafe {
            self.insert_length(index, node.0);
        }
        self.children().insert(index, node.1);
        self.set_parent_links(index..);
    }

    unsafe fn split_and_insert_left(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T, B, C>,
    ) -> RawNodeWithLen<T, B, C> {
        let split_index = Self::UNDERFULL_LEN;

        let new_sibling_node = InternalNode::<T, B, C>::new();
        let mut new_sibling =
            unsafe { Node::<ownership::Mut, H, T, B, C>::new(new_sibling_node.cast()) };

        unsafe {
            self.split_lengths(split_index, new_sibling.reborrow());
            self.children().split(split_index, new_sibling.children());
            self.insert_fitting(index, node)
        };

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

        let mut new_sibling =
            unsafe { Node::<ownership::Mut, H, T, B, C>::new(new_sibling_node.cast()) };

        unsafe {
            self.split_lengths(split_index, new_sibling.reborrow());
            self.children().split(split_index, new_sibling.children());
            new_sibling.insert_fitting(index - split_index, node);
        }

        new_sibling.set_parent_links(0..);
        RawNodeWithLen(new_sibling.sum_lens(), new_sibling_node.cast())
    }

    fn set_parent_links(&mut self, range: RangeFrom<usize>) {
        for (i, n) in self.children()[range.clone()].iter_mut().enumerate() {
            unsafe {
                (*n.as_ptr()).parent = Some(self.node_ptr());
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
