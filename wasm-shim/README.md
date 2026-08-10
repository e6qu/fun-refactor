# wasm-shim

A libc small enough for tree-sitter, for `wasm32-unknown-unknown`.

The grammars are C. Their `alloc.h` includes `<stdio.h>` and `<stdlib.h>`, and a few
scanners want `<wctype.h>`. Targeting `wasm32-unknown-unknown` there is no libc at
all: wasi-sdk's headers exist but refuse the target outright, and adding a WASI shim
to the browser would mean shipping a syscall layer to run a parser that makes no
syscalls.

So this declares exactly what the grammars reference and nothing else. The
implementations are in `src/wasm_libc.rs`, backed by Rust's allocator and its
`compiler_builtins` memory intrinsics — no allocator of our own, no syscalls, and
nothing that can drift from what the C actually calls, because a missing symbol is a
link error instead of a runtime surprise.

Not used by any other build. `CFLAGS_wasm32_unknown_unknown` points at it, and that
variable is set in one place: `tools/build-wasm.sh`.
