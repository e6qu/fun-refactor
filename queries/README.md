# queries/

Per-language fact extraction, interpreted by the generic engine in `src/extract.rs`.
Adding a language means writing queries here, not Rust.

The capture conventions are documented at the top of `src/extract.rs`; it is the
authority, since it is what reads these files.

## Derivation

These were written by consulting the query files each grammar ships upstream, which
are the canonical statement of a grammar's node names. Those are vendored under
`vendor/tree-sitter-queries/` with their licences and checksums, so the derivation is
recorded rather than implicit — see `vendor/README.md`.

When a grammar is bumped in `Cargo.toml`, re-run `python3 vendor/vendor.py` and read
the diff before trusting these files. A grammar that renames a node does not break the
build; it makes a pattern here silently stop matching, and the only warning is that
diff plus the per-language test suites.
