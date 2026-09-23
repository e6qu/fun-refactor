# Development guide

`fr` requires stable Rust, Python 3 and the toolchains used by the language fixtures. Lean tests
use the repository's pinned Lake project. Zig and Go tests keep their caches under `target` by
default.

## Build and test

Build the native CLI with the locked dependency graph:

```sh
cargo build --locked
cargo run --locked --features cli -- --version
```

The check script defines the same gates as CI. The default and WASM lanes run on pull requests.
The deep lane runs repository-scale audits, every Lean case and external consumer replays.

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Rust test fan-out in the default, deep and Lean-kernel gates defaults to two. Lean processes use the
same worker bound. Override either value with a positive integer when the machine has suitable
capacity.

```sh
FR_LEAN_JOBS=1 LEAN_NUM_THREADS=1 tools/check.sh default
FR_LEAN_JOBS=1 LEAN_NUM_THREADS=1 tools/check.sh deep
```

Install and check the Python SDK in its own environment:

```sh
python3 -m venv sdk/python/.venv
sdk/python/.venv/bin/python -m pip install -U pip
sdk/python/.venv/bin/python -m pip install -e 'sdk/python[test]'
sdk/python/.venv/bin/python -m pip install 'ty==0.0.80'
sdk/python/.venv/bin/pytest sdk/python/tests
sdk/python/.venv/bin/ty check sdk/python/src
```

Use targeted tests while developing, then run the complete affected lane before pushing. The
portable agent skill has an executable example checker:

```sh
cargo test --test agent_skill
python3 tools/check-agent-skill.py --fr target/debug/fr
```

## Source provenance

Dependency upgrades use the newest stable release that has been public for at least 24 hours.

Tree-sitter grammars are pinned Cargo dependencies. Repository query files live under `queries/`,
and [their README](../queries/README.md) records conventions. Vendored source and licence details
live in the [vendor guide](../vendor/README.md). Do not download parsers or grammars at runtime.

The agent evaluation fixtures under `tests/agent-eval/` retain their own source, licence and digest
records. See the [evaluation guide](evaluations.md) before updating a retained artifact.

## Add or extend a language

1. Add or update the parser identity and extension mapping.
2. Add the tree-sitter grammar and its provenance.
3. Define symbol, scope and reference queries for the admitted syntax.
4. Add capability predicates with a reason for every unsupported cell.
5. Add fixtures that exercise accepted syntax, ambiguous cases and refusals.
6. Add writer or transformation support only for constructs the IR can represent.
7. Regenerate and check the capability matrix.
8. Update the durable language references and the defect ledger.

The capability matrix reports predicate support. Test coverage separately proves that fixtures
executed each advertised cell.

## Release

Release Please prepares the version and changelog pull request. A version tag starts the release
workflow, which builds native archives and the WASM package, attaches checksums and verifies the
published shapes. Keep installation instructions independent of a specific version number.

Before release, run the default, WASM and deep lanes and inspect the generated archive locally.

```sh
cargo test --test release
cargo test --test packaging
```
