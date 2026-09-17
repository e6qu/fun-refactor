# Browser playground

The playground loads a public GitHub repository into the browser and runs the WASM build of `fr`
without a server. It does not send changes back to GitHub or hold an access token.

## Run locally

Install the web dependencies and start the development server:

```sh
npm install
npm run dev
```

`npm run dev` expects the generated module under `web/src/wasm`. Build it first with either
`WASI_SDK` or `WASM_CLANG`, then generate the JavaScript bindings:

```sh
WASI_SDK=/path/to/wasi-sdk ../tools/build-wasm.sh
wasm-bindgen --target web --out-dir src/wasm ../target/wasm32-unknown-unknown/release/fun_refactor.wasm
```

The `wasm-bindgen` CLI must match the version in `Cargo.lock` because the generated ABI is unstable.
CI reads that version from the lockfile.

Build the static site under `docs/playground` with:

```sh
npm run build
```

## Select languages

The bundle includes Monaco and each selected tree-sitter grammar. Build a smaller language set by
passing features to the WASM script:

```sh
FEATURES=wasm,lang-go,lang-typescript,lang-python ../tools/build-wasm.sh
```

Measure the current artifacts when setting a download budget because the selected grammars can
change between releases.

## Boundaries

The playground edits only its in-memory workspace. It supports browser transaction history, undo,
redo, initial-workspace restore and patch export. Native Git staging, commits and worktrees remain
native CLI operations.

Repository loading is limited to 400 files and 6 MiB, smallest files first. The interface reports
every omitted file. See `src/github.ts` for the loader contract.

