# vendor/

Third-party material copied into this repository, with the provenance needed to say
where it came from and what may be done with it.

Nothing here is compiled into the binary. It is reference material and derivation
evidence: the query files in `queries/` were written by reading these, and recording
that is the difference between a derivation and an uncredited copy.

## What is here

`tree-sitter-queries/<language>/` — the query files each grammar ships upstream
(`highlights.scm`, `tags.scm`, `locals.scm`, `injections.scm`), plus that grammar's
licence text.

These are the canonical statement of a grammar's node names. Every `queries/*/facts.scm`
in this repository was written by consulting them, so they are both the reference a
maintainer needs and the source our own rules derive from.

`MANIFEST.toml` — generated provenance for every file: language, crate, the exact
version Cargo resolved, upstream repository, SPDX licence, and a SHA-256 of each file.

For the node names a grammar actually produces — which is what a query has to match,
and where the per-language bugs come from — `cargo run --example dump -- <file>`
prints the parse tree with its field names.

## The rules

**Every artifact records its source, its pin, its licence and a checksum.** A file
with no entry in `MANIFEST.toml` is not vendored, it is stray.

**The pin is the crate version, not a git commit.** A tree-sitter grammar crate is a
mirror of a repository, and the crate version is what Cargo resolves and the build
actually compiles. Pinning a commit would record something we do not build.

**Licences must be compatible with AGPL-3.0-or-later**, which is this project's
licence. MIT and Apache-2.0 are; a copyleft licence with different terms is not, and
`tests/vendor.rs` fails the build rather than letting one in quietly.

**Nothing vendored is compiled.** `Cargo.toml` excludes this directory from the
published package, and no `include_str!` points into it. If that ever changes, the
licence obligations change with it — an MIT file compiled into an AGPL binary needs
its notice preserved, which is a different conversation from reference material.

**Absence is recorded.** A grammar that ships no queries gets a `MANIFEST.toml` entry
saying so, because "nothing here" and "nobody looked" are different facts.

## Refreshing

```
python3 vendor/vendor.py
```

Idempotent: it rewrites every file and regenerates the manifest from the crates Cargo
has already resolved, so the diff shows exactly what changed upstream. Run it after
bumping a grammar in `Cargo.toml`, and read the diff — a grammar that renamed a node
is how `queries/*/facts.scm` silently stops matching.

`cargo test --test vendor` verifies the manifest against the files on disk: a checksum
that no longer matches, a file with no entry, or an entry with no file all fail.
