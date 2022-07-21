use core::ffi::c_void;

use objc2::runtime::Class;

// This is defined in CoreFoundation, but we don't emit a link attribute
// here because it is already linked via Foundation.
//
// Although this is a "private" (underscored) symbol, it is directly
// referenced in Objective-C binaries. So it's safe for us to reference.
extern "C" {
    pub static __CFConstantStringClassReference: Class;
}

// From `CFString.c`:
// https://github.com/apple-oss-distributions/CF/blob/CF-1153.18/CFString.c#L184-L212
// > !!! Note: Constant CFStrings use the bit patterns:
// > C8 (11001000 = default allocator, not inline, not freed contents; 8-bit; has NULL byte; doesn't have length; is immutable)
// > D0 (11010000 = default allocator, not inline, not freed contents; Unicode; is immutable)
// > The bit usages should not be modified in a way that would effect these bit patterns.
//
// The 7 byte is the `CFTypeID` of `CFStringRef`.
const FLAGS_ASCII: usize = 0x07_C8;
const FLAGS_UTF16: usize = 0x07_D0;

#[repr(C)]
pub struct CFStringAscii {
    isa: &'static Class,
    flags: usize,
    data: *const u8,
    len: usize,
}

// Required to place in a `static`.
unsafe impl Sync for CFStringAscii {}

impl CFStringAscii {
    pub const fn new(isa: &'static Class, data: *const u8, len: usize) -> Self {
        Self {
            isa,
            data,
            len,
            flags: FLAGS_ASCII,
        }
    }

    pub const fn as_ptr(&self) -> *const c_void {
        self as *const Self as *const c_void
    }
}

#[repr(C)]
pub struct CFStringUtf16 {
    isa: &'static Class,
    flags: usize,
    data: *const u16,
    len: usize,
}

// Required to place in a `static`.
unsafe impl Sync for CFStringUtf16 {}

impl CFStringUtf16 {
    pub const fn new(isa: &'static Class, data: *const u16, len: usize) -> Self {
        Self {
            isa,
            data,
            len,
            flags: FLAGS_UTF16,
        }
    }

    pub const fn as_ptr(&self) -> *const c_void {
        self as *const Self as *const c_void
    }
}

/// Returns `s` with any 0 byte at the end removed.
pub const fn trim_trailing_nul(s: &str) -> &[u8] {
    match s.as_bytes() {
        [b @ .., 0] => b,
        b => b,
    }
}

/// Returns `true` if `bytes` is entirely ASCII with no interior nulls.
pub const fn is_ascii(bytes: &[u8]) -> bool {
    let mut i = 0;
    loop {
        if i == bytes.len() {
            return true;
        }

        let byte = bytes[i];
        if !byte.is_ascii() || byte == 0 {
            return false;
        }

        i += 1;
    }
}

pub struct Utf16Char {
    pub repr: [u16; 2],
    pub len: usize,
}

impl Utf16Char {
    const fn encode(ch: u32) -> Self {
        if ch <= 0xffff {
            Self {
                repr: [ch as u16, 0],
                len: 1,
            }
        } else {
            let payload = ch - 0x10000;
            let hi = (payload >> 10) | 0xd800;
            let lo = (payload & 0x3ff) | 0xdc00;
            Self {
                repr: [hi as u16, lo as u16],
                len: 2,
            }
        }
    }

    #[cfg(test)]
    pub fn as_slice(&self) -> &[u16] {
        &self.repr[..self.len]
    }
}

pub struct EncodeUtf16Iter {
    str: &'static [u8],
    index: usize,
}

impl EncodeUtf16Iter {
    pub const fn new(str: &'static [u8]) -> Self {
        Self { str, index: 0 }
    }

    pub const fn next(self) -> Option<(Self, Utf16Char)> {
        if self.index >= self.str.len() {
            None
        } else {
            let (index, ch) = decode_utf8(self.str, self.index);
            Some((Self { index, ..self }, Utf16Char::encode(ch)))
        }
    }
}

// (&str bytes, index) -> (new index, decoded char)
const fn decode_utf8(s: &[u8], i: usize) -> (usize, u32) {
    let b0 = s[i];
    match b0 {
        // one-byte seq
        0b0000_0000..=0b0111_1111 => {
            let decoded = b0 as u32;
            (i + 1, decoded)
        }
        // two-byte seq
        0b1100_0000..=0b1101_1111 => {
            let decoded = ((b0 as u32 & 0x1f) << 6) | (s[i + 1] as u32 & 0x3f);
            (i + 2, decoded)
        }
        // 3 byte seq
        0b1110_0000..=0b1110_1111 => {
            let decoded = ((b0 as u32 & 0x0f) << 12)
                | ((s[i + 1] as u32 & 0x3f) << 6)
                | (s[i + 2] as u32 & 0x3f);
            (i + 3, decoded)
        }
        // 3 byte seq
        0b1111_0000..=0b1111_0111 => {
            let decoded = ((b0 as u32 & 0x07) << 18)
                | ((s[i + 1] as u32 & 0x3f) << 12)
                | ((s[i + 2] as u32 & 0x3f) << 6)
                | (s[i + 3] as u32 & 0x3f);
            (i + 4, decoded)
        }
        // continuation bytes, or never-valid bytes.
        0b1000_0000..=0b1011_1111 | 0b1111_1000..=0b1111_1111 => {
            #[allow(unconditional_panic)]
            {
                // replace this with unreachable!() when possible in const fn.
                const NOT_POSSIBLE_FOR_VALID_UTF8: [(); 0] = [];
                let _ = NOT_POSSIBLE_FOR_VALID_UTF8[0];
            }
            (s.len(), 0xfffd)
        }
    }
}

/// Creates an [`NSString`][`crate::NSString`] from a static string.
///
///
/// # Examples
///
/// This macro takes a either a `"string"` literal or `const` string slice as
/// the argument:
///
/// ```
/// use objc2_foundation::ns_string;
/// let hello = ns_string!("hello");
/// assert_eq!(hello.to_string(), "hello");
///
/// const WORLD: &str = "world";
/// let world = ns_string!(WORLD);
/// assert_eq!(world.to_string(), WORLD);
/// ```
///
/// The result of this macro can even be used to create `static` values:
///
/// ```
/// # use objc2_foundation::{ns_string, NSString};
/// static WORLD: &NSString = ns_string!("world");
///
/// assert_eq!(WORLD.to_string(), "world");
/// ```
///
/// Note that the result cannot be used in a `const` because it refers to
/// static data outside of this library.
///
///
/// # Unicode Strings
///
/// In Objective-C, non-ASCII strings are UTF-16. However, Rust strings are
/// UTF-8.
///
/// This macro transcodes non-ASCII strings to UTF-16:
///
/// ```
/// # use objc2_foundation::{ns_string, NSString};
/// static HELLO_RU: &NSString = ns_string!("Привет");
///
/// assert_eq!(HELLO_RU.to_string(), "Привет");
/// ```
///
/// Note that because this is implemented with `const` evaluation, massive
/// strings can increase compile time and even hit the `const` evaluation
/// limit.
///
///
/// # Null-Terminated Strings
///
/// If the input string already ends with a 0 byte, then this macro does not
/// append one.
///
/// ```
/// # use objc2_foundation::ns_string;
/// let cstr = ns_string!("example\0");
/// let normal = ns_string!("example");
///
/// assert_eq!(cstr, normal);
/// ```
///
/// Interior null bytes are allowed and are not stripped:
///
/// ```
/// # use objc2_foundation::ns_string;
/// let example = ns_string!("exa\0mple");
/// assert_eq!(example.to_string(), "exa\0mple");
/// ```
///
///
/// # Runtime Cost
///
/// None.
///
/// The result is equivalent to `@"string"` syntax in Objective-C.
///
/// Because of that, this should be preferred over [`NSString::from_str`]
/// where possible.
///
/// [`NSString::from_str`]: crate::NSString::from_str
///
///
/// # Compile-time Cost
///
/// Minimal.
///
/// This is implemented entirely with `const` evaluation. It is not a
/// procedural macro that requires dependencies for parsing.
#[macro_export]
macro_rules! ns_string {
    ($s:expr) => {{
        // Note that this always uses full paths to items from `$crate`. This
        // does not import any items because doing so could cause ambiguity if
        // the same names are exposed at the call site of this macro.
        //
        // The only names directly used are expressions, whose names shadow any
        // other names outside of this macro.

        let cfstring_ptr: *const $crate::__core::ffi::c_void = {
            // Remove any trailing null early.
            const INPUT: &[u8] = $crate::__string_macro::trim_trailing_nul($s);

            if $crate::__string_macro::is_ascii(INPUT) {
                // The ASCII bytes with a trailing null byte.
                #[repr(C)]
                struct Ascii {
                    data: [u8; INPUT.len()],
                    nul: u8,
                }

                const ASCII: Ascii = Ascii {
                    data: unsafe { *$crate::__core::mem::transmute::<_, &_>(INPUT.as_ptr()) },
                    nul: 0,
                };

                const ASCII_ARRAY: &[u8; INPUT.len() + 1] =
                    unsafe { $crate::__core::mem::transmute(&ASCII) };

                #[link_section = "__DATA,__cfstring,regular"]
                static CFSTRING: $crate::__string_macro::CFStringAscii =
                    $crate::__string_macro::CFStringAscii::new(
                        unsafe { &$crate::__string_macro::__CFConstantStringClassReference },
                        ASCII_ARRAY.as_ptr(),
                        // The length does not include the trailing null.
                        INPUT.len(),
                    );

                CFSTRING.as_ptr()
            } else {
                // The full UTF-16 contents along with the written length.
                const UTF16_FULL: (&[u16; INPUT.len()], usize) = {
                    let mut out = [0u16; INPUT.len()];
                    let mut iter = $crate::__string_macro::EncodeUtf16Iter::new(INPUT);
                    let mut written = 0;

                    while let Some((state, chars)) = iter.next() {
                        iter = state;
                        out[written] = chars.repr[0];
                        written += 1;

                        if chars.len > 1 {
                            out[written] = chars.repr[1];
                            written += 1;
                        }
                    }

                    (&{ out }, written)
                };

                // The written UTF-16 contents with a trailing null code point.
                #[repr(C)]
                struct Utf16 {
                    data: [u16; UTF16_FULL.1],
                    nul: u16,
                }

                const UTF16: Utf16 = Utf16 {
                    data: unsafe {
                        *$crate::__core::mem::transmute::<_, &_>(UTF16_FULL.0.as_ptr())
                    },
                    nul: 0,
                };

                const UTF16_ARRAY: &[u16; UTF16_FULL.1 + 1] =
                    unsafe { $crate::__core::mem::transmute(&UTF16) };

                #[link_section = "__DATA,__cfstring,regular"]
                static CFSTRING: $crate::__string_macro::CFStringUtf16 =
                    $crate::__string_macro::CFStringUtf16::new(
                        unsafe { &$crate::__string_macro::__CFConstantStringClassReference },
                        UTF16_ARRAY.as_ptr(),
                        // The length does not include the trailing null.
                        UTF16_FULL.1,
                    );

                CFSTRING.as_ptr()
            }
        };

        union Cast<T: 'static> {
            pointer: *const T,
            reference: &'static T,
        }

        #[allow(unused_unsafe)]
        let ns_string: &$crate::NSString = unsafe {
            Cast {
                pointer: cfstring_ptr.cast(),
            }
            .reference
        };

        ns_string
    }};
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;
    use crate::NSString;

    #[test]
    fn test_decode_utf8() {
        for c in '\u{0}'..=core::char::MAX {
            let mut buf;
            for off in 0..4 {
                // Ensure we see garbage if we read outside bounds.
                buf = [0xff; 8];
                let len = c.encode_utf8(&mut buf[off..(off + 4)]).len();
                let (end_idx, decoded) = decode_utf8(&buf, off);
                assert_eq!(
                    (end_idx, decoded),
                    (off + len, c as u32),
                    "failed for U+{code:04X} ({ch:?}) encoded as {buf:#x?} over {range:?}",
                    code = c as u32,
                    ch = c,
                    buf = &buf[off..(off + len)],
                    range = off..(off + len),
                );
            }
        }
    }

    #[test]
    fn encode_utf16() {
        for c in '\u{0}'..=core::char::MAX {
            assert_eq!(
                c.encode_utf16(&mut [0u16; 2]),
                Utf16Char::encode(c as u32).as_slice(),
                "failed for U+{:04X} ({:?})",
                c as u32,
                c
            );
        }
    }

    #[test]
    fn ns_string() {
        macro_rules! test {
            ($($s:expr,)+) => {$({
                static STRING: &NSString = ns_string!($s);
                assert_eq!(STRING.to_string(), $s);
            })+};
        }

        test! {
            "asdf",
            "🦀",
            "🏳️‍🌈",
            "𝄞music",
            "abcd【e】fg",
            "abcd⒠fg",
            "ääääh",
            "lööps, bröther?",
            "\u{fffd} \u{fffd} \u{fffd}",
            "讓每個人都能打造出。",
        }
    }
}
