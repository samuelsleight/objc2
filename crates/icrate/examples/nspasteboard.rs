//! Read from the global pasteboard, and write a new string into it.
//!
//! Works on macOS 10.7+
#![deny(unsafe_op_in_unsafe_fn)]

use icrate::AppKit::{NSPasteboard, NSPasteboardTypeString};
use icrate::Foundation::{NSArray, NSCopying, NSString};
use objc2::rc::Id;
use objc2::runtime::{Class, Object, ProtocolObject};
use objc2::ClassType;

/// Simplest implementation
pub fn get_text_1(pasteboard: &NSPasteboard) -> Option<Id<NSString>> {
    unsafe { pasteboard.stringForType(NSPasteboardTypeString) }
}

/// More complex implementation using `readObjectsForClasses:options:`,
/// intended to show how some patterns might require more knowledge of
/// nitty-gritty details.
pub fn get_text_2(pasteboard: &NSPasteboard) -> Option<Id<NSString>> {
    // The NSPasteboard API is a bit weird, it requires you to pass classes as
    // objects, which `icrate::Foundation::NSArray` was not really made for -
    // so we convert the class to an `Object` type instead.
    //
    // TODO: Investigate and find a better way to express this in `objc2`.
    let string_class = {
        let cls: *const Class = NSString::class();
        let cls = cls as *mut Object;
        unsafe { Id::new(cls).unwrap() }
    };
    let class_array = NSArray::from_vec(vec![string_class]);
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&class_array, None) };

    let obj: *const Object = objects?.first()?;
    // And this part is weird as well, since we now have to convert the object
    // into an NSString, which we know it to be since that's what we told
    // `readObjectsForClasses:options:`.
    let obj = obj as *mut NSString;
    Some(unsafe { Id::retain(obj) }.unwrap())
}

pub fn set_text(pasteboard: &NSPasteboard, text: &NSString) {
    let _ = unsafe { pasteboard.clearContents() };
    let obj = ProtocolObject::from_id(text.copy());
    let objects = NSArray::from_vec(vec![obj]);
    let res = unsafe { pasteboard.writeObjects(&objects) };
    if !res {
        panic!("Failed writing to pasteboard");
    }
}

fn main() {
    let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
    let impl_1 = get_text_1(&pasteboard);
    let impl_2 = get_text_2(&pasteboard);
    println!("Pasteboard text from implementation 1 was: {impl_1:?}");
    println!("Pasteboard text from implementation 2 was: {impl_2:?}");
    assert_eq!(impl_1, impl_2);

    let s = NSString::from_str("Hello, world!");
    set_text(&pasteboard, &s);
    println!("Now the pasteboard text should be: {s:?}");
    assert_eq!(Some(s), get_text_1(&pasteboard));
}
