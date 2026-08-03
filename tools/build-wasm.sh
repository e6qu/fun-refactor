#!/usr/bin/env bash
# Build the analysis core for the browser.
#
# `wasm32-unknown-unknown` has no libc, and the tree-sitter grammars are C that
# expects one. wasm-shim/include declares exactly what they call and src/wasm_libc.rs
# implements it, so the sysroot is deliberately *not* on the include path: pointing at
# wasi's headers is what produced "<wasi/api.h> is only supported on WASI platforms".
#
# Set WASM_CLANG, or WASI_SDK (only its clang is used, never its sysroot), or have a
# wasm-capable clang on PATH.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Any clang with the WebAssembly backend will do. Apple's will not — it is built
# without that target — so a Mac needs wasi-sdk (used only for its compiler, never
# its sysroot). Linux distributions ship an LLVM with every target enabled, which is
# why CI just says `clang`.
clang="${WASM_CLANG:-}"
if [ -z "$clang" ] && [ -n "${WASI_SDK:-}" ]; then
  clang="$WASI_SDK/bin/clang"
fi
if [ -z "$clang" ] && command -v clang >/dev/null; then
  clang="$(command -v clang)"
fi
if [ -z "$clang" ] || ! "$clang" --print-targets 2>/dev/null | grep -q wasm32; then
  echo "need a clang that can emit wasm32." >&2
  echo "  Linux: apt install clang" >&2
  echo "  macOS: fetch wasi-sdk and set WASI_SDK, or set WASM_CLANG" >&2
  echo "  https://github.com/WebAssembly/wasi-sdk/releases" >&2
  exit 1
fi
ar="$(dirname "$clang")/llvm-ar"
[ -x "$ar" ] || ar="$(command -v llvm-ar || command -v ar)"

export CC_wasm32_unknown_unknown="$clang"
export AR_wasm32_unknown_unknown="$ar"
# `-include` rather than trusting each scanner to include what it calls: several
# rely on implicit declarations, which C99 removed and clang now rejects.
clang_include="$("$clang" -print-resource-dir)/include"
export CFLAGS_wasm32_unknown_unknown="--target=wasm32-unknown-unknown -nostdinc \
  -isystem $here/wasm-shim/include -isystem $clang_include -fno-builtin -DNDEBUG \
  -include stdbool.h -include fr_shim.h"

cd "$here"
# Every grammar unless told otherwise. `--no-default-features` drops the terminal's
# dependencies *and* the language bundle they come with, and a browser build with no
# grammars parses nothing — which is a confusing way to find out.
features="${FEATURES:-wasm,lang-all}"
cargo build --release --target wasm32-unknown-unknown \
  --no-default-features --features "$features" "$@"
echo "built: target/wasm32-unknown-unknown/release/fun_refactor.wasm"
