# Qualify the files users install

[PLAN.md](../PLAN.md#proposed-pr-sequence) owns product completion. This check covers packaging and installed workflows.
It does not establish agent benefit or close missing feature obligations. PR 5 remains at 0 of 96 live attempts,
and the original M3 150-second gate remains open. Reserved validation projects stay unopened.

The installed-consumer checks landed in [#629](https://github.com/e6qu/fun-refactor/pull/629).
Its [full CI](https://github.com/e6qu/fun-refactor/actions/runs/44487429807) and
[four native platforms, WASM and SDK qualification](https://github.com/e6qu/fun-refactor/actions/runs/44487429786) passed.

## One build, checked before upload

[Product qualification](../.github/workflows/product-qualification.yml) runs on packaging and SDK pull requests
and can be dispatched manually. It calls the same [packaging workflow](../.github/workflows/package-product.yml)
as a release, using an exact source commit and its package version.
Checker scripts come from the reviewed workflow revision, so manual completion of an older release
does not depend on that release already containing the new scripts. Product builds and the browser
test harness still come from the selected source commit; a failed consumer check prevents upload.

The workflow builds four native archives, the browser archive, and the Python wheel and source archive.
Each job checks the finished package before retaining it as a GitHub Actions artifact. Receipts identify
the archive or distribution by SHA-256 and record the product and checker commits.
Candidate downloads expire after 14 days. The separate `qualification-evidence` artifact retains
the manifest and consumer receipts for 90 days, subject to repository retention settings.

After every package job passes, a bundle job requires the seven downloads, six checksum files and six receipts.
It verifies the file set, hashes, versions, source/checker commits and required consumer checks, then writes
`fun-refactor-vVERSION.qualification.json`. Missing or unexpected files, failed checks and mixed revisions refuse.
The `qualified-product` artifact contains those 19 files and their manifest.

The release job downloads only `qualified-product`, verifies the manifest against its expected version and commits,
then attaches the checked files to the selected release. It never rebuilds a wheel after checking it.
Existing release selection still requires a published source tag; manual completion accepts only the latest
release with no assets. Ordinary pushes do not start release builds.

| Download | Consumer check |
| --- | --- |
| Linux x86-64 and ARM64, macOS x86-64 and ARM64 | Extract the archive, check required documentation and skill files, execute the packaged binary, and require the selected version |
| Native CLI and installed Python wheel | Check a source-linked call graph, a preview that leaves files unchanged, rename and Python behavior, refreshed graph digests, undo/redo, patch application and reversal in a fresh Git repository, and stale-preview refusal |
| Python source archive | Rebuild it outside the checkout, install its wheel in a second clean virtual environment, and repeat the SDK consumer checks |
| Browser archive | Load the extracted JavaScript/WASM pair into a fresh test harness and run API, reviewed-edit, patch-delivery and session-recovery suites |

SDK consumers run with isolated Python imports and no checkout on their import path. Both installed distributions
must match the native version. A native execution failure fails qualification, including the Intel macOS build;
checking its file architecture alone cannot pass. The browser harness supplies the UI source used by its tests,
while all engine code comes from the archive under test.

`tools/test-release-packages.py` checks rejection of changed checksums, wrong filenames, unsafe archive paths,
duplicate members, links, missing files, failed native execution and ambiguous SDK distributions.
`tools/test-product-bundle.py` checks incomplete bundles, mixed revisions, substituted receipts,
altered manifests and failed or missing native, browser and SDK consumer checks.
These controls and release-selection controls run before candidate builds. Full CI remains a separate gate.

## Verify a downloaded bundle

Put the release's downloads, checksums, receipts and qualification manifest in one directory, then run:

```sh
python3 -B tools/check-product-bundle.py --dist downloads --version VERSION \
  --source-commit SOURCE_COMMIT --checker-commit CHECKER_COMMIT
```

Use the source and checker commits selected by the reviewed workflow run. Both must be full Git object IDs.
The command checks recorded consumer outcomes and every file digest; it does not rerun the consumers.
The manifest records integrity and provenance within that workflow's trust boundary. It is not a signature
or an independent authentication of an arbitrary bundle. Stable three-part release versions are supported.

## What remains outside this check

This small consumer exercises integration and packaging. The full language and operation matrix still belongs to
[analysis](analysis-coverage.md), [refactoring](refactoring-coverage.md), [proof](proof-coverage.md),
and [interface](interface-coverage.md) coverage and their hosted tests. A packaged rename does not prove every
transformation or arbitrary-program equivalence. Linux runs on each target architecture; Intel macOS execution
on an Apple silicon runner uses the runner's translation support, not independent Intel hardware.

Complete-product release qualification also requires the roadmap's outstanding feature obligations, examples,
release replay, full CI and live comparisons. Package receipts alone cannot declare that milestone complete.
