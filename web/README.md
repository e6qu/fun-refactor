# Browser playground

Try a change before touching your checkout. For example, load a public project, rename a function, inspect the diff and export a patch. The browser keeps those edits in memory; applying the patch to your checkout is a separate step.

The playground uses WASM, a compiled format the browser can execute.
Its virtual workspace holds editable files in browser memory; it is separate from your local Git checkout.
A diff shows proposed changes, and a patch carries those changes to another checkout.
For a trial rename, inspect the diff in the browser before exporting and testing the patch locally.

The built-in Signals project is sample source for trying the tool across languages.
Its project notes and runbook illustrate documentation you might inspect in an application; use the setup below to run this playground itself.

On the shared workstation, follow the [local work limits](../docs/development.md#working-on-a-shared-desktop). Full browser builds run on GitHub.

The playground loads a public GitHub repository into the browser and runs the WASM build of `fr`
without a server. It does not send changes back to GitHub or hold an access token.

## Run locally

Use a dedicated development machine or GitHub runner for the full build.
From the repository root, enter the web folder and install its dependencies:

```sh
cd web
npm install
```

The server needs the generated module under `web/src/wasm`.
The following commands continue from the `web` folder. Replace `/path/to/wasi-sdk` with the installed compiler toolkit's folder.
Build with `WASI_SDK` or explicitly set `WASM_CLANG` to the compiler you are using, then generate the JavaScript interface:

```sh
WASI_SDK=/path/to/wasi-sdk ../tools/build-wasm.sh
wasm-bindgen --target web --out-dir src/wasm ../target/wasm32-unknown-unknown/release/fun_refactor.wasm
```

Install a `wasm-bindgen` CLI matching the version in `Cargo.lock`. The generated interface must agree with the compiled module.
CI reads that version from the lockfile.

Start the server from the same folder:

```sh
npm run dev
```

Open the address it prints. Load the Signals sample, select a function and inspect a rename preview before exporting its patch.

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

For example, a checkout helper may call a file that the loader omitted. A caller list in that
partial workspace cannot establish the full project's affected code. Inspect the omitted files and
test the exported patch in the complete checkout before using it.

