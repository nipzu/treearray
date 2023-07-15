use core::marker::PhantomData;

// TODO: should this be covariant?
pub struct Immut<'a>(PhantomData<&'a ()>);
pub struct Mut<'a>(PhantomData<&'a mut ()>);

pub unsafe trait Reference<T> {}

unsafe impl<'a, T: 'a> Reference<T> for Immut<'a> {}
unsafe impl<'a, T: 'a> Reference<T> for Mut<'a> {}
