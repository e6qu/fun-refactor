//! The C library tree-sitter needs, in Rust, for `wasm32-unknown-unknown`.
//!
//! The grammars are C and call a handful of libc functions. That target has no libc,
//! and the alternative — building for WASI and shipping a syscall shim to the browser
//! — would mean emulating a filesystem to run a parser that never touches one.
//!
//! Tree-sitter 0.27 supplies its portable libc. This crate fills the narrow character
//! functions and process exit that current grammar scanners still call.

use core::ffi::c_int;

// Tree-sitter supplies the allocator, the `mem*`/`str*` family and wide character
// functions. Defining them here as well causes duplicate-symbol link errors.

// `abort` too: stdlib.c defines it, and it traps, which is the only sensible thing a
// wasm module can do in place of ending a process.

#[no_mangle]
pub extern "C" fn exit(_status: c_int) -> ! {
    core::arch::wasm32::unreachable()
}

// tree-sitter's own wasm headers declare the allocation and memory functions, so
// only what they leave out is defined here. There is no `fprintf`: a variadic
// definition is not stable in Rust, and nothing in the linked grammars reaches the
// diagnostic path — if one ever does, the link fails and says so, which is better
// than a silent stub.

// Character classes. A scanner asks these about identifier characters, so the ASCII
// answer is the whole of what the grammars rely on; anything above it is treated as a
// letter, which is what every one of these grammars wants for an identifier.

fn ascii(c: c_int) -> Option<u8> {
    (0..=0x7f).contains(&c).then_some(c as u8)
}

macro_rules! class {
    ($name:ident, $test:expr, $above_ascii:expr) => {
        #[no_mangle]
        pub extern "C" fn $name(c: c_int) -> c_int {
            match ascii(c) {
                Some(b) => c_int::from($test(b)),
                None => c_int::from($above_ascii),
            }
        }
    };
}

class!(isalpha, |b: u8| b.is_ascii_alphabetic(), true);
class!(isalnum, |b: u8| b.is_ascii_alphanumeric(), true);
class!(isdigit, |b: u8| b.is_ascii_digit(), false);
class!(islower, |b: u8| b.is_ascii_lowercase(), false);
class!(isupper, |b: u8| b.is_ascii_uppercase(), false);
class!(isspace, |b: u8| b.is_ascii_whitespace(), false);
class!(ispunct, |b: u8| b.is_ascii_punctuation(), false);
class!(isxdigit, |b: u8| b.is_ascii_hexdigit(), false);

/// Case conversion, ASCII only — an HTML scanner folds tag names with it.
#[no_mangle]
pub extern "C" fn toupper(c: c_int) -> c_int {
    match ascii(c) {
        Some(b) => c_int::from(b.to_ascii_uppercase()),
        None => c,
    }
}

#[no_mangle]
pub extern "C" fn tolower(c: c_int) -> c_int {
    match ascii(c) {
        Some(b) => c_int::from(b.to_ascii_lowercase()),
        None => c,
    }
}
