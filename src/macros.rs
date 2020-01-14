//! This module contains macros shared accross the crate.

/// A macro for getting the compiler to assert that a type is always `Unpin`.
///
/// # Examples
///
/// An `Option<T>` is only `Unpin` if `T: Unpin` so the following assertion will
/// fail to compile since `for<T> Option<T>: Unpin` does not hold.
/// ```compile_fail
/// # use ethcontract::assert_unpin;
/// assert_unpin!([T] Option<T>);
/// ```
///
/// However, `Box<T>` is `Unpin` regardless of `T`, so the following assertion
/// will not cause a compilation error.
/// ```no_run
/// # use ethcontract::assert_unpin;
/// assert_unpin!([T] Box<T>);
/// ```
macro_rules! assert_unpin {
    ([$($g:tt)*] $type:ty) => {
        fn __assert_unpin<$($g)*>(value: $type) {
            fn __assert_unpin_inner(_: impl Unpin) {}
            __assert_unpin_inner(value);
        }
    };
}
