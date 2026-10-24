# Build the C parsers for the browser

The browser runs the WebAssembly build of `fr`. Its C parsers need declarations for functions such as memory allocation.
The `wasm32-unknown-unknown` target has no operating-system C library, so these headers declare the small set of interfaces the parsers use.

A header declares a function's name and types. The implementation supplies its behavior.
For example, declaring an allocator lets a parser compile; an implementation must still provide the matching function when the program links.

The implementations live in [the Rust browser runtime](../crates/wasm-libc/src/lib.rs).
They use Rust's allocator and memory operations. This does not add operating-system calls to the browser.

[The WASM build script](../tools/build-wasm.sh) sets `CFLAGS_wasm32_unknown_unknown` to use these headers.
Other targets use their normal C library. Full browser builds run on a dedicated runner; follow the [development limits](../docs/development.md#working-on-a-shared-desktop).

If linking reports a missing C function, check both its declaration here and its implementation in the Rust runtime.
A missing implementation can stop linking. Successful linking alone does not establish that the C declaration and Rust behavior agree.
Run the browser tests after changing either side.
