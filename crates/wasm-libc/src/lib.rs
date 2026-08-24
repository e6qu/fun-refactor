//! The C library tree-sitter needs, in Rust, for `wasm32-unknown-unknown`.
//!
//! Its own crate because `fun-refactor` forbids unsafe code, and raw pointers are the
//! entire job here. Keeping the exception in one small crate means the guarantee for
//! the analysis is unqualified and this file can be reviewed on its own terms.
//!
//! The grammars are C and call a handful of libc functions. That target has no libc,
//! and the alternative — building for WASI and shipping a syscall shim to the browser
//! — would mean emulating a filesystem to run a parser that never touches one.
//!
//! So this defines the symbols the C references, backed by Rust's allocator. Only
//! what is actually called: a missing symbol is a link error, which is the right way
//! to find out that a grammar wants something new.
//!
//! Allocation carries its own size. `free` and `realloc` are given only a pointer,
//! but Rust's deallocator needs the layout back, so every block is prefixed with a
//! header holding its size and the pointer handed to C is offset past it. The header
//! is aligned to `MAX_ALIGN` so the payload satisfies any alignment C can ask for.

#![allow(clippy::missing_safety_doc)]

use core::ffi::{c_char, c_int};

// The allocator and the `mem*`/`str*` family come from tree-sitter-language's own
// wasm sources (`wasm/src/stdlib.c` and `string.c`), which some grammars compile in.
// Defining them here as well is a duplicate-symbol link error, and theirs is the one
// the grammars were built against.

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
    ($name:ident, $wide:ident, $test:expr, $above_ascii:expr) => {
        #[no_mangle]
        pub extern "C" fn $name(c: c_int) -> c_int {
            match ascii(c) {
                Some(b) => c_int::from($test(b)),
                None => c_int::from($above_ascii),
            }
        }
        #[no_mangle]
        pub extern "C" fn $wide(c: c_int) -> c_int {
            $name(c)
        }
    };
}

class!(isalpha, iswalpha, |b: u8| b.is_ascii_alphabetic(), true);
class!(isalnum, iswalnum, |b: u8| b.is_ascii_alphanumeric(), true);
class!(isdigit, iswdigit, |b: u8| b.is_ascii_digit(), false);
class!(islower, iswlower, |b: u8| b.is_ascii_lowercase(), false);
class!(isupper, iswupper, |b: u8| b.is_ascii_uppercase(), false);
class!(isspace, iswspace, |b: u8| b.is_ascii_whitespace(), false);
class!(ispunct, iswpunct, |b: u8| b.is_ascii_punctuation(), false);
class!(isxdigit, iswxdigit, |b: u8| b.is_ascii_hexdigit(), false);

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

#[no_mangle]
pub extern "C" fn towupper(c: c_int) -> c_int {
    toupper(c)
}

#[no_mangle]
pub extern "C" fn towlower(c: c_int) -> c_int {
    tolower(c)
}

#[no_mangle]
pub extern "C" fn iswblank(c: c_int) -> c_int {
    c_int::from(c == b' ' as c_int || c == b'\t' as c_int)
}

// `memcpy`, `memmove`, `memset` and `memcmp` come from Rust's compiler_builtins and
// must not be defined again here. The string functions below are not.

#[no_mangle]
pub unsafe extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let (x, y) = (*a.add(i), *b.add(i));
        if x != y {
            return c_int::from(x as u8) - c_int::from(y as u8);
        }
        if x == 0 {
            return 0;
        }
        i += 1;
    }
}

/// Hand the grammars' allocator a region of its own, once.
///
/// `wasm/src/stdlib.c` from tree-sitter-language is a bump allocator written for
/// tree-sitter's *sandbox*, where the host calls `reset_heap` before running an
/// external scanner. Nothing calls it here, so its `next` pointer starts at NULL and
/// the first allocation writes to address zero — over the module's own data. The
/// symptoms were a different trap per input ("memory access out of bounds" on
/// TypeScript, "function signature mismatch" on Rust) with Go and YAML working,
/// which is what a corrupted indirect-call table looks like from the outside.
///
/// It also refuses to grow past `MAX_HEAP_SIZE`, 4 MB, so it cannot host a Rust
/// program. Instead it gets a leaked 4 MB arena and Rust keeps its own allocator:
/// the two never share an address, and the cap that made it unusable as *the*
/// allocator is exactly what keeps it inside the arena.
///
/// External scanners allocate a few kilobytes between them, so 4 MB is generous.
pub fn init_scanner_heap() {
    use core::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    const ARENA: usize = 4 * 1024 * 1024;
    let arena = vec![0u8; ARENA].into_boxed_slice();
    let start = Box::leak(arena).as_mut_ptr();
    unsafe { reset_heap(start as *mut core::ffi::c_void) };
}

extern "C" {
    fn reset_heap(new_heap_start: *mut core::ffi::c_void);
}

/// Give tree-sitter's core Rust's allocator.
///
/// The bump arena above is bounded at 4 MB by design — it was written to hold an
/// external scanner's scratch, not a parser, a query and an index. Compiling the
/// TypeScript query alone exhausted it, and tree-sitter's response to a failed
/// allocation is `abort()`, which in wasm is a trap with no message.
///
/// `set_allocator` is the supported way to answer that: the library allocates through
/// these instead, which are Rust's allocator with a size header, and the arena is left
/// to the scanners that actually call `malloc` themselves.
///
/// Through the Rust binding, which keeps its own copy of the free function and uses it
/// for every buffer the C side hands back. Setting only the C library's allocator
/// leaves that copy pointing at the scanners' arena, so a string the parser allocated
/// and the binding freed would cross two heaps. `Node::to_sexp` does exactly that, and
/// one call to it tripped the arena's own assertion in the browser.
pub fn use_rust_allocator_in_tree_sitter() {
    unsafe {
        tree_sitter::set_allocator(
            Some(fr_malloc),
            Some(fr_calloc),
            Some(fr_realloc),
            Some(fr_free),
        )
    };
}

/// The strictest alignment any of this asks for.
const MAX_ALIGN: usize = 16;

#[repr(C, align(16))]
struct Header {
    size: usize,
}

const HEADER: usize = core::mem::size_of::<Header>();

fn layout_for(total: usize) -> core::alloc::Layout {
    core::alloc::Layout::from_size_align(total, MAX_ALIGN).expect("a layout for a block")
}

/// `free` and `realloc` are handed only a pointer, but Rust's deallocator needs the
/// layout back, so every block carries its size immediately before the payload.
unsafe extern "C" fn fr_malloc(size: usize) -> *mut core::ffi::c_void {
    let Some(total) = size.checked_add(HEADER) else {
        return core::ptr::null_mut();
    };
    let base = std::alloc::alloc(layout_for(total));
    if base.is_null() {
        return core::ptr::null_mut();
    }
    (base as *mut Header).write(Header { size: total });
    base.add(HEADER) as *mut core::ffi::c_void
}

unsafe extern "C" fn fr_calloc(count: usize, size: usize) -> *mut core::ffi::c_void {
    let Some(bytes) = count.checked_mul(size) else {
        return core::ptr::null_mut();
    };
    let ptr = fr_malloc(bytes);
    if !ptr.is_null() {
        core::ptr::write_bytes(ptr as *mut u8, 0, bytes);
    }
    ptr
}

unsafe extern "C" fn fr_realloc(
    ptr: *mut core::ffi::c_void,
    size: usize,
) -> *mut core::ffi::c_void {
    if ptr.is_null() {
        return fr_malloc(size);
    }
    let base = (ptr as *mut u8).sub(HEADER);
    let old_total = (base as *mut Header).read().size;
    let Some(new_total) = size.checked_add(HEADER) else {
        return core::ptr::null_mut();
    };
    let grown = std::alloc::realloc(base, layout_for(old_total), new_total);
    if grown.is_null() {
        return core::ptr::null_mut();
    }
    (grown as *mut Header).write(Header { size: new_total });
    grown.add(HEADER) as *mut core::ffi::c_void
}

unsafe extern "C" fn fr_free(ptr: *mut core::ffi::c_void) {
    if ptr.is_null() {
        return;
    }
    let base = (ptr as *mut u8).sub(HEADER);
    let total = (base as *mut Header).read().size;
    std::alloc::dealloc(base, layout_for(total));
}
