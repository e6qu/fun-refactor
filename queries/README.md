# Teach fr to recognize declarations and uses

When adding language support, tell the extractor which syntax nodes declare a function and which use its name.
For example, a missed call capture can leave a rename’s caller list incomplete. The query files below define those patterns.

## Build and reference details

Per-language fact extraction, interpreted by the generic engine in `src/extract.rs`.
Adding a language means writing queries here, not Rust.

The capture conventions are documented at the top of `src/extract.rs`; it is the
authority, since it is what reads these files.

## Derivation

These were written by consulting the query files each grammar ships upstream, which
are the canonical statement of a grammar's node names. Those are vendored under
`vendor/tree-sitter-queries/` with their licences and checksums, so the derivation is
recorded and not implicit. See `vendor/README.md`.

When a grammar is bumped in `Cargo.toml`, re-run `python3 vendor/vendor.py` and read
the diff before trusting these files. A grammar that renames a node does not break the
build; it makes a pattern here silently stop matching, and the only warning is that
diff plus the per-language test suites.
