use core::fmt;
use core::hash;
use core::marker::PhantomData;
use core::ptr::NonNull;

use crate::encode::{Encoding, RefEncode};
use crate::rc::{autoreleasepool_leaking, Id};
use crate::runtime::__nsstring::nsstring_to_str;
use crate::runtime::{NSObjectProtocol, Object};
use crate::{Message, ProtocolType};

/// An internal helper trait for [`ProtocolObject`].
///
///
/// # Safety
///
/// This is meant to be a sealed trait, and should not be implemented outside
/// of the [`extern_protocol!`] macro.
///
/// [`extern_protocol!`]: crate::extern_protocol
pub unsafe trait ImplementedBy<T: ?Sized + Message> {
    #[doc(hidden)]
    const __INNER: ();
}

/// An object representing any object that implements a specified protocol.
///
/// Objective-C has [a feature][protocol-type-checking] where you can write
/// `id<MyProtocol>`, and then work with the protocol as-if it was an object;
/// this is very similar to `dyn` traits in Rust!
///
/// If we could customize how `dyn Trait` works, then this struct would not
/// have been necessary; however, `dyn Trait` is a wide pointer with overhead,
/// which this struct helps avoid.
///
/// If the trait `T` inherits [`NSObjectProtocol`], this will implement common
/// traits like `Debug`, `PartialEq`, `Eq` and `Hash`.
///
/// [protocol-type-checking]: https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ObjectiveC/Chapters/ocProtocols.html#//apple_ref/doc/uid/TP30001163-CH15-TPXREF151
///
///
/// # Example
///
/// Convert an object `MyObject` that implements the a protocol `MyProtocol`
/// into a [`ProtocolObject`] for working with the protocol in a type-erased
/// way.
///
/// ```
/// use objc2::runtime::ProtocolObject;
/// use objc2::rc::Id;
/// # use objc2::runtime::NSObject as MyObject;
/// # use objc2::runtime::NSObjectProtocol as MyProtocol;
///
/// let obj: Id<MyObject> = MyObject::new();
/// let proto: &ProtocolObject<dyn MyProtocol> = ProtocolObject::from_ref(&*obj);
/// let proto: Id<ProtocolObject<dyn MyProtocol>> = ProtocolObject::from_id(obj);
/// ```
#[doc(alias = "id")]
#[repr(C)]
pub struct ProtocolObject<P: ?Sized + ProtocolType> {
    inner: Object,
    p: PhantomData<P>,
}

// SAFETY: The type is `#[repr(C)]` and `Object` internally
unsafe impl<P: ?Sized + ProtocolType> RefEncode for ProtocolObject<P> {
    const ENCODING_REF: Encoding = Encoding::Object;
}

// SAFETY: The type is `Object` internally, and is mean to be messaged as-if
// it's an object.
unsafe impl<P: ?Sized + ProtocolType> Message for ProtocolObject<P> {}

impl<P: ?Sized + ProtocolType> ProtocolObject<P> {
    /// Get an immutable type-erased reference from a type implementing a
    /// protocol.
    #[inline]
    pub fn from_ref<T: Message>(obj: &T) -> &Self
    where
        P: ImplementedBy<T>,
    {
        let ptr: NonNull<T> = NonNull::from(obj);
        let ptr: NonNull<Self> = ptr.cast();
        // SAFETY: Implementer ensures that the object conforms to the
        // protocol; so converting the reference here is safe.
        unsafe { ptr.as_ref() }
    }

    /// Get a mutable type-erased reference from a type implementing a
    /// protocol.
    #[inline]
    pub fn from_mut<T: Message>(obj: &mut T) -> &mut Self
    where
        P: ImplementedBy<T>,
    {
        let ptr: NonNull<T> = NonNull::from(obj);
        let mut ptr: NonNull<Self> = ptr.cast();
        // SAFETY: Same as `as_protocol`.
        //
        // Since the reference came from a mutable reference to start with,
        // returning a mutable reference here is safe (the lifetime of the
        // returned reference is bound to the input).
        unsafe { ptr.as_mut() }
    }

    /// Get a type-erased object from a type implementing a protocol.
    #[inline]
    pub fn from_id<T: Message>(obj: Id<T>) -> Id<Self>
    where
        P: ImplementedBy<T> + 'static,
        T: 'static,
    {
        // SAFETY:
        // - The type can be represented as the casted-to type.
        // - Both types are `'static` (this could maybe be relaxed a bit, but
        //   let's be on the safe side)!
        unsafe { Id::cast::<Self>(obj) }
    }
}

impl<P: ?Sized + ProtocolType + NSObjectProtocol> PartialEq for ProtocolObject<P> {
    #[inline]
    #[doc(alias = "isEqual:")]
    fn eq(&self, other: &Self) -> bool {
        self.__isEqual(other)
    }
}

impl<P: ?Sized + ProtocolType + NSObjectProtocol> Eq for ProtocolObject<P> {}

impl<P: ?Sized + ProtocolType + NSObjectProtocol> hash::Hash for ProtocolObject<P> {
    #[inline]
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.__hash().hash(state);
    }
}

impl<P: ?Sized + ProtocolType + NSObjectProtocol> fmt::Debug for ProtocolObject<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = self.__description();

        match description {
            // Attempt to format description string
            Some(description) => {
                // We use a leaking autorelease pool since often the string
                // will be UTF-8, and in that case the pool will be
                // irrelevant. Also, it allows us to pass the formatter into
                // the pool (since it may contain a pool internally that it
                // assumes is current when writing).
                autoreleasepool_leaking(|pool| {
                    // SAFETY: `description` selector is guaranteed to always
                    // return an instance of `NSString`.
                    let s = unsafe { nsstring_to_str(&description, pool) };
                    fmt::Display::fmt(s, f)
                })
            }
            // If description was `NULL`, use `Object`'s `Debug` impl instead
            None => {
                let obj: &Object = &self.inner;
                fmt::Debug::fmt(obj, f)
            }
        }
    }
}

impl<P, T> AsRef<ProtocolObject<T>> for ProtocolObject<P>
where
    P: ?Sized + ProtocolType,
    T: ?Sized + ProtocolType + ImplementedBy<ProtocolObject<P>>,
{
    #[inline]
    fn as_ref(&self) -> &ProtocolObject<T> {
        ProtocolObject::from_ref(self)
    }
}

impl<P, T> AsMut<ProtocolObject<T>> for ProtocolObject<P>
where
    P: ?Sized + ProtocolType,
    T: ?Sized + ProtocolType + ImplementedBy<ProtocolObject<P>>,
{
    #[inline]
    fn as_mut(&mut self) -> &mut ProtocolObject<T> {
        ProtocolObject::from_mut(self)
    }
}

// TODO: Maybe iplement Borrow + BorrowMut?

#[cfg(test)]
#[allow(clippy::missing_safety_doc)]
mod tests {
    use alloc::format;
    use core::mem::ManuallyDrop;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::mutability::Mutable;
    use crate::runtime::{NSObject, NSObjectProtocol};
    use crate::{declare_class, extern_methods, extern_protocol, ClassType};

    extern_protocol!(
        unsafe trait Foo {
            #[method(foo)]
            fn foo_class();

            #[method(foo)]
            fn foo_instance(&self);
        }

        unsafe impl ProtocolType for dyn Foo {}
    );

    extern_protocol!(
        unsafe trait Bar: NSObjectProtocol {
            #[method(bar)]
            fn bar_class();

            #[method(bar)]
            fn bar_instance(&self);
        }

        unsafe impl ProtocolType for dyn Bar {}
    );

    extern_protocol!(
        unsafe trait FooBar: Foo + Bar {
            #[method(foobar)]
            fn foobar_class();

            #[method(foobar)]
            fn foobar_instance(&self);
        }

        unsafe impl ProtocolType for dyn FooBar {}
    );

    extern_protocol!(
        unsafe trait FooFooBar: Foo + FooBar {
            #[method(foofoobar)]
            fn foofoobar_class();

            #[method(foofoobar)]
            fn foofoobar_instance(&self);
        }

        unsafe impl ProtocolType for dyn FooFooBar {}
    );

    declare_class!(
        #[derive(Debug, PartialEq, Eq, Hash)]
        struct DummyClass;

        unsafe impl ClassType for DummyClass {
            type Super = NSObject;
            type Mutability = Mutable;
            const NAME: &'static str = "ProtocolTestsDummyClass";
        }
    );

    extern_methods!(
        unsafe impl DummyClass {
            #[method_id(new)]
            fn new() -> Id<Self>;
        }
    );

    unsafe impl NSObjectProtocol for DummyClass {}
    unsafe impl Foo for DummyClass {}
    unsafe impl Bar for DummyClass {}
    unsafe impl FooBar for DummyClass {}
    // unsafe impl FooFooBar for DummyClass {}

    #[test]
    fn impl_traits() {
        assert_impl_all!(NSObject: NSObjectProtocol);
        assert_impl_all!(ProtocolObject<NSObject>: NSObjectProtocol);
        assert_not_impl_any!(ProtocolObject<dyn Foo>: NSObjectProtocol);
        assert_impl_all!(ProtocolObject<dyn Bar>: NSObjectProtocol);
        assert_impl_all!(ProtocolObject<dyn FooBar>: NSObjectProtocol);
        assert_impl_all!(ProtocolObject<dyn FooFooBar>: NSObjectProtocol);
        assert_impl_all!(DummyClass: NSObjectProtocol);

        assert_not_impl_any!(NSObject: Foo);
        assert_not_impl_any!(ProtocolObject<NSObject>: Foo);
        assert_impl_all!(ProtocolObject<dyn Foo>: Foo);
        assert_not_impl_any!(ProtocolObject<dyn Bar>: Foo);
        assert_impl_all!(ProtocolObject<dyn FooBar>: Foo);
        assert_impl_all!(ProtocolObject<dyn FooFooBar>: Foo);
        assert_impl_all!(DummyClass: Foo);

        assert_not_impl_any!(NSObject: Bar);
        assert_not_impl_any!(ProtocolObject<NSObject>: Bar);
        assert_not_impl_any!(ProtocolObject<dyn Foo>: Bar);
        assert_impl_all!(ProtocolObject<dyn Bar>: Bar);
        assert_impl_all!(ProtocolObject<dyn FooBar>: Bar);
        assert_impl_all!(ProtocolObject<dyn FooFooBar>: Bar);
        assert_impl_all!(DummyClass: Bar);

        assert_not_impl_any!(NSObject: FooBar);
        assert_not_impl_any!(ProtocolObject<NSObject>: FooBar);
        assert_not_impl_any!(ProtocolObject<dyn Foo>: FooBar);
        assert_not_impl_any!(ProtocolObject<dyn Bar>: FooBar);
        assert_impl_all!(ProtocolObject<dyn FooBar>: FooBar);
        assert_impl_all!(ProtocolObject<dyn FooFooBar>: FooBar);
        assert_impl_all!(DummyClass: FooBar);

        assert_not_impl_any!(NSObject: FooFooBar);
        assert_not_impl_any!(ProtocolObject<NSObject>: FooFooBar);
        assert_not_impl_any!(ProtocolObject<dyn Foo>: FooFooBar);
        assert_not_impl_any!(ProtocolObject<dyn Bar>: FooFooBar);
        assert_not_impl_any!(ProtocolObject<dyn FooBar>: FooFooBar);
        assert_impl_all!(ProtocolObject<dyn FooFooBar>: FooFooBar);
        assert_not_impl_any!(DummyClass: FooFooBar);
    }

    #[test]
    fn convertible() {
        let mut obj = DummyClass::new();
        let foobar: &ProtocolObject<dyn FooBar> = ProtocolObject::from_ref(&*obj);
        let foobar: &ProtocolObject<dyn FooBar> = ProtocolObject::from_ref(foobar);

        let _bar: &ProtocolObject<dyn Bar> = ProtocolObject::from_ref(foobar);
        let bar: &ProtocolObject<dyn Bar> = ProtocolObject::from_ref(&*obj);
        let bar: &ProtocolObject<dyn Bar> = ProtocolObject::from_ref(bar);

        let _foo: &ProtocolObject<dyn Foo> = ProtocolObject::from_ref(foobar);
        let foo: &ProtocolObject<dyn Foo> = ProtocolObject::from_ref(&*obj);
        let _foo: &ProtocolObject<dyn Foo> = ProtocolObject::from_ref(foo);

        let _nsobject: &ProtocolObject<NSObject> = ProtocolObject::from_ref(foobar);
        let _nsobject: &ProtocolObject<NSObject> = ProtocolObject::from_ref(bar);
        let nsobject: &ProtocolObject<NSObject> = ProtocolObject::from_ref(&*obj);
        let _nsobject: &ProtocolObject<NSObject> = ProtocolObject::from_ref(nsobject);

        let _foobar: &mut ProtocolObject<dyn FooBar> = ProtocolObject::from_mut(&mut *obj);
        let _foobar: Id<ProtocolObject<dyn FooBar>> = ProtocolObject::from_id(obj);
    }

    #[test]
    fn test_traits() {
        use core::hash::Hasher;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hash;

        let obj = DummyClass::new();
        let obj2 = DummyClass::new();

        let foobar: &ProtocolObject<dyn FooBar> = ProtocolObject::from_ref(&*obj);
        let foobar2: &ProtocolObject<dyn FooBar> = ProtocolObject::from_ref(&*obj2);

        assert_eq!(
            format!("{obj:?}"),
            format!(
                "DummyClass {{ __superclass: {:?} }}",
                ManuallyDrop::new(foobar)
            ),
        );
        assert_eq!(obj == obj2, foobar == foobar2);

        let mut hashstate_a = DefaultHasher::new();
        let mut hashstate_b = DefaultHasher::new();

        obj.hash(&mut hashstate_a);
        foobar.hash(&mut hashstate_b);

        assert_eq!(hashstate_a.finish(), hashstate_b.finish());
    }

    // We use `debug_assertions` here just because it's something that we know
    // our CI already tests.
    extern_protocol!(
        #[cfg(debug_assertions)]
        unsafe trait CfgTest {}

        #[cfg(debug_assertions)]
        unsafe impl ProtocolType for dyn CfgTest {}
    );

    #[test]
    #[cfg(debug_assertions)]
    fn test_meta() {
        if false {
            let _protocol = <dyn CfgTest>::protocol();
        }
    }
}
