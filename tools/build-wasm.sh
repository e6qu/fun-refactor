#!/usr/bin/env bash
# Build the analysis core for the browser.
#
# `wasm32-unknown-unknown` has no libc, and the tree-sitter grammars are C that
# expects one. wasm-shim/include declares exactly what they call and src/wasm_libc.rs
# implements it, so the sysroot is deliberately *not* on the include path: pointing at
# wasi's headers is what produced "<wasi/api.h> is only supported on WASI platforms".
#
# WASI_SDK may point at any wasi-sdk install; it is used only for its clang, which is
# an LLVM with the wasm backend that Apple's clang does not have.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk="${WASI_SDK:-}"
if [ -z "$sdk" ] || [ ! -x "$sdk/bin/clang" ]; then
  echo "set WASI_SDK to a wasi-sdk install (needs \$WASI_SDK/bin/clang)" >&2
  echo "  https://github.com/WebAssembly/wasi-sdk/releases" >&2
  exit 1
fi

export CC_wasm32_unknown_unknown="$sdk/bin/clang"
export AR_wasm32_unknown_unknown="$sdk/bin/llvm-ar"
# `-include` rather than trusting each scanner to include what it calls: several
# rely on implicit declarations, which C99 removed and clang now rejects.
clang_include="$(echo "$sdk"/lib/clang/*/include)"
export CFLAGS_wasm32_unknown_unknown="--target=wasm32-unknown-unknown -nostdinc \
  -isystem $here/wasm-shim/include -isystem $clang_include -fno-builtin -DNDEBUG \
  -include stdbool.h -include fr_shim.h"

cd "$here"
cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm "$@"
echo "built: target/wasm32-unknown-unknown/release/fun_refactor.wasm"
