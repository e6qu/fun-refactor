# Check the source of copied reference material

Use this directory when updating a grammar or the patterns that recognize its declarations and uses.
The upstream query files provide examples of the parser's syntax names. Our own patterns live in [queries](../queries/README.md).

“Vendored” means copied from another project into this repository.
Each copied file has a recorded source, version, license and checksum: a fingerprint used to detect changed contents.
These records let a maintainer check which upstream material informed a pattern.

## What is here

| Path | Contents | How it is used |
| --- | --- | --- |
| `tree-sitter-queries/<language>/` | Upstream query files and license text | Reference when writing or updating our queries |
| `MANIFEST.toml` | Language, resolved crate version, repository, license identifier and file fingerprints | Check sources and detect missing or altered files |
| `vendor.py` | Refresh program | Copy reference files from the grammar crates resolved by Cargo |

This directory is excluded from the published package and is not compiled into `fr`.
Compiled grammar sources live in [grammars](../grammars/README.md).
If a grammar ships no query files, the manifest records that absence.

The manifest pins the crate version Cargo resolves, rather than a repository revision that the build does not use.
Its SPDX field uses a standard short license identifier, such as `MIT`.
Preserve the copied license texts. The license policy enforced by [tests/vendor.rs](../tests/vendor.rs) applies to additions and updates.

## Refresh after a grammar update

Run this on a dedicated runner after changing a grammar dependency in `Cargo.toml`:

```sh
python3 vendor/vendor.py
cargo test --test vendor
```

The script rewrites the copied files and manifest from the resolved crates.
Review that diff, then update and test our own query patterns.
For example, an upstream grammar renaming a call node can make a caller disappear from our results even if compilation succeeds.

The test checks fingerprints, missing manifest entries and missing files.
It does not establish that our queries recognize every relevant declaration or caller.
Use the [language checks](../docs/development.md#add-or-extend-a-language) for those behaviors.
Follow the [shared workstation limits](../docs/development.md#working-on-a-shared-desktop) for local inspection.
