# web/ — the playground

A public GitHub repository, loaded into the tab and refactored there. No server: the
analysis is this crate compiled to WebAssembly, and the repository is fetched from
GitHub's API by the browser.

```
npm install
npm run dev          # needs web/src/wasm — see below
npm run build        # writes ../docs/playground, which CI publishes
```

`src/wasm/` is generated and not committed:

```
WASI_SDK=/path/to/wasi-sdk ../tools/build-wasm.sh     # or WASM_CLANG=…
wasm-bindgen --target web --out-dir src/wasm \
  ../target/wasm32-unknown-unknown/release/fun_refactor.wasm
```

The wasm-bindgen CLI must match the `wasm-bindgen` version in `Cargo.lock` exactly —
the two share an unstable ABI, and a mismatch fails with a schema-version error that
says so. CI reads the version out of the lockfile for this reason.

## What a first visit costs

Roughly 1.7 MB of WebAssembly and 1.0 MB of Monaco, gzipped. The analysis is the
larger half because it carries fifteen tree-sitter grammars, each a parse table of
about a megabyte before compression. A build with fewer is one flag away —
`FEATURES=wasm,lang-go,lang-typescript,lang-python ../tools/build-wasm.sh` comes to
1.16 MB — but the saving did not seem worth asking a visitor which languages they
were about to need.

## What it does not do

It does not write to GitHub, and it holds no token. A refactoring here edits the copy
in the tab; the diff is what you take away. Loading is capped at 400 files and 6 MB,
smallest first, and whatever is left out is reported rather than quietly dropped —
see `src/github.ts`.
