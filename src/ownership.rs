use core::marker::PhantomData;

// TODO: should this be covariant?
pub struct Immut<'a>(PhantomData<&'a ()>);
pub struct Mut<'a>(PhantomData<&'a mut ()>);
pub struct Owned;

pub unsafe trait Mutable<T>: Ownership<T> {}
unsafe impl<'a, T: 'a> Mutable<T> for Mut<'a> {}
unsafe impl<T> Mutable<T> for Owned {}

pub unsafe trait Reference<'a, T: 'a>: Ownership<T> {
    type RefTy<U: 'a>;
    fn as_ref<U>(r: &Self::RefTy<U>) -> &U;
}
unsafe impl<'a, T: 'a> Reference<'a, T> for Immut<'a> {
    type RefTy<U: 'a> = &'a U;
    fn as_ref<U>(r: &Self::RefTy<U>) -> &U {
        r
    }
}
unsafe impl<'a, T: 'a> Reference<'a, T> for Mut<'a> {
    type RefTy<U: 'a> = &'a mut U;
    fn as_ref<U>(r: &Self::RefTy<U>) -> &U {
        r
    }
}

pub unsafe trait Ownership<T> {}
unsafe impl<'a, T: 'a> Ownership<T> for Immut<'a> {}
unsafe impl<'a, T: 'a> Ownership<T> for Mut<'a> {}
unsafe impl<T> Ownership<T> for Owned {}
