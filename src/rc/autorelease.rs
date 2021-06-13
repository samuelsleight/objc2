use crate::runtime::{objc_autoreleasePoolPop, objc_autoreleasePoolPush};
use std::os::raw::c_void;

/// An Objective-C autorelease pool.
///
/// The pool is drained when dropped.
///
/// This is not `Send`, since `objc_autoreleasePoolPop` must be called on the
/// same thread.
///
/// And this is not `Sync`, since you can only autorelease a reference to a
/// pool on the current thread.
///
/// See [the clang documentation][clang-arc] and
/// [this apple article][memory-mgmt] for more information on automatic
/// reference counting.
///
/// [clang-arc]: https://clang.llvm.org/docs/AutomaticReferenceCounting.html
/// [memory-mgmt]: https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/MemoryMgmt/Articles/MemoryMgmt.html
pub struct AutoreleasePool {
    context: *mut c_void,
}

/// ```rust,compile_fail
/// use objc::rc::AutoreleasePool;
/// fn needs_sync<T: Send>() {}
/// needs_sync::<AutoreleasePool>();
/// ```
/// ```rust,compile_fail
/// use objc::rc::AutoreleasePool;
/// fn needs_send<T: Send>() {}
/// needs_send::<AutoreleasePool>();
/// ```
#[cfg(doctest)]
pub struct AutoreleasePoolNotSendNorSync;

impl AutoreleasePool {
    /// Construct a new autorelease pool.
    ///
    /// Use the [`autoreleasepool`] block for a safe alternative.
    ///
    /// # Safety
    ///
    /// The caller must ensure that when handing out `&'p AutoreleasePool` to
    /// functions that this is the innermost pool.
    ///
    /// Additionally, the pools must be dropped in the same order they were
    /// created.
    #[doc(alias = "objc_autoreleasePoolPush")]
    unsafe fn new() -> Self {
        // TODO: Make this function pub when we're more certain of the API
        Self {
            context: objc_autoreleasePoolPush(),
        }
    }

    // TODO: Add helper functions to ensure (with debug_assertions) that the
    // pool is innermost when its lifetime is tied to a reference.
}

impl Drop for AutoreleasePool {
    /// Drains the autoreleasepool.
    ///
    /// The [clang documentation] says that `@autoreleasepool` blocks are not
    /// drained when exceptions occur because:
    ///
    /// > Not draining the pool during an unwind is apparently required by the
    /// > Objective-C exceptions implementation.
    ///
    /// This was true in the past, but since [revision `371`] of
    /// `objc-exception.m` (ships with MacOS 10.5) the exception is now
    /// retained when `@throw` is encountered.
    ///
    /// Hence it is safe to drain the pool when unwinding.
    ///
    /// [clang documentation]: https://clang.llvm.org/docs/AutomaticReferenceCounting.html#autoreleasepool
    /// [revision `371`]: https://opensource.apple.com/source/objc4/objc4-371/runtime/objc-exception.m.auto.html
    #[doc(alias = "objc_autoreleasePoolPop")]
    fn drop(&mut self) {
        unsafe { objc_autoreleasePoolPop(self.context) }
    }
}

/// A trait for the sole purpose of ensuring we can't pass an `&AutoreleasePool`
/// through to the closure inside `autoreleasepool`
#[cfg(feature = "unstable_autoreleasesafe")]
pub unsafe auto trait AutoreleaseSafe {}
#[cfg(feature = "unstable_autoreleasesafe")]
impl !AutoreleaseSafe for AutoreleasePool {}

#[cfg(feature = "unstable_autoreleasesafe")]
macro_rules! fn_autoreleasepool {
    {$(#[$fn_meta:meta])* $v:vis fn $fn:ident($f:ident) $b:block} => {
        $(#[$fn_meta])*
        $v fn $fn<T, F>($f: F) -> T
        where
            for<'p> F: FnOnce(&'p AutoreleasePool) -> T + AutoreleaseSafe,
        {
            $b
        }
    }
}

#[cfg(not(feature = "unstable_autoreleasesafe"))]
macro_rules! fn_autoreleasepool {
    {$(#[$fn_meta:meta])* $v:vis fn $fn:ident($f:ident) $b:block} => {
        $(#[$fn_meta])*
        $v fn $fn<T, F>($f: F) -> T
        where
            for<'p> F: FnOnce(&'p AutoreleasePool) -> T,
        {
            $b
        }
    }
}

fn_autoreleasepool!(
    /// Execute `f` in the context of a new autorelease pool. The pool is
    /// drained after the execution of `f` completes.
    ///
    /// This corresponds to `@autoreleasepool` blocks in Objective-C and
    /// Swift.
    ///
    /// The pool is passed as a reference to the enclosing function to give it
    /// a lifetime parameter that autoreleased objects can refer to.
    ///
    /// The given reference must not be used in an inner `autoreleasepool`,
    /// doing so will be a compile error in a future release. You can test
    /// this guarantee with the `unstable_autoreleasesafe` crate feature on
    /// nightly Rust.
    ///
    /// So using `autoreleasepool` is unsound right now because of this
    /// specific problem.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```rust
    /// use objc::{class, msg_send};
    /// use objc::rc::{autoreleasepool, AutoreleasePool};
    /// use objc::runtime::Object;
    ///
    /// fn needs_lifetime_from_pool<'p>(_pool: &'p AutoreleasePool) -> &'p mut Object {
    ///     let obj: *mut Object = unsafe { msg_send![class!(NSObject), new] };
    ///     let obj: *mut Object = unsafe { msg_send![obj, autorelease] };
    ///     // SAFETY: Lifetime bounded by the pool
    ///     unsafe { &mut *obj }
    /// }
    ///
    /// autoreleasepool(|pool| {
    ///     // Create `obj` and autorelease it to the pool
    ///     let obj = needs_lifetime_from_pool(pool);
    ///     // ... use `obj` here
    ///     // `obj` is deallocated when the pool ends
    /// });
    /// ```
    ///
    /// Fails to compile because `obj` does not live long enough for us to
    /// safely take it out of the pool:
    ///
    /// ```rust,compile_fail
    /// # use objc::{class, msg_send};
    /// # use objc::rc::{autoreleasepool, AutoreleasePool};
    /// # use objc::runtime::Object;
    /// #
    /// # fn needs_lifetime_from_pool<'p>(_pool: &'p AutoreleasePool) -> &'p mut Object {
    /// #     let obj: *mut Object = unsafe { msg_send![class!(NSObject), new] };
    /// #     let obj: *mut Object = unsafe { msg_send![obj, autorelease] };
    /// #     unsafe { &mut *obj }
    /// # }
    /// #
    /// let obj = autoreleasepool(|pool| {
    ///     let obj = needs_lifetime_from_pool(pool);
    ///     // Use `obj`
    ///     obj
    /// });
    /// ```
    ///
    /// Incorrect usage which causes undefined behaviour:
    ///
    #[cfg_attr(feature = "unstable_autoreleasesafe", doc = "```rust,compile_fail")]
    #[cfg_attr(not(feature = "unstable_autoreleasesafe"), doc = "```rust")]
    /// # use objc::{class, msg_send};
    /// # use objc::rc::{autoreleasepool, AutoreleasePool};
    /// # use objc::runtime::Object;
    /// #
    /// # fn needs_lifetime_from_pool<'p>(_pool: &'p AutoreleasePool) -> &'p mut Object {
    /// #     let obj: *mut Object = unsafe { msg_send![class!(NSObject), new] };
    /// #     let obj: *mut Object = unsafe { msg_send![obj, autorelease] };
    /// #     unsafe { &mut *obj }
    /// # }
    /// #
    /// autoreleasepool(|outer_pool| {
    ///     let obj = autoreleasepool(|inner_pool| {
    ///         let obj = needs_lifetime_from_pool(outer_pool);
    ///         obj
    ///     });
    ///     // `obj` can wrongly be used here because it's lifetime was
    ///     // assigned to the outer pool, even though it was released by the
    ///     // inner pool already.
    /// });
    /// ```
    pub fn autoreleasepool(f) {
        let pool = unsafe { AutoreleasePool::new() };
        f(&pool)
    }
);

#[cfg(all(test, feature = "unstable_autoreleasesafe"))]
mod tests {
    use super::AutoreleaseSafe;
    use crate::runtime::Object;

    fn requires_autoreleasesafe<T: AutoreleaseSafe>() {}

    #[test]
    fn test_autoreleasesafe() {
        requires_autoreleasesafe::<usize>();
        requires_autoreleasesafe::<*mut Object>();
        requires_autoreleasesafe::<&mut Object>();
    }
}
