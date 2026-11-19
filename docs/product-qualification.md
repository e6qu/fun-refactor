# Qualify the files users install

[PLAN.md](../PLAN.md#proposed-pr-sequence) owns product completion. This check covers packaging and installed workflows.
It does not establish agent benefit or close missing feature obligations. PR 5 remains at 0 of 96 live attempts,
and the original M3 150-second gate remains open. Reserved validation projects stay unopened.

## One build, checked before upload

[Product qualification](../.github/workflows/product-qualification.yml) runs on packaging and SDK pull requests
and can be dispatched manually. It calls the same [packaging workflow](../.github/workflows/package-product.yml)
as a release, using an exact source commit and its package version.
Checker scripts come from the reviewed workflow revision, so manual completion of an older release
does not depend on that release already containing the new scripts. Product builds and the browser
test harness still come from the selected source commit; a failed consumer check prevents upload.

The workflow builds four native archives, the browser archive, and the Python wheel and source archive.
Each job checks the finished package before retaining it as a GitHub Actions artifact. Receipts identify
the archive or distribution by SHA-256. Candidate artifacts expire after 14 days and are not releases.

The release workflow waits for every package job, downloads those artifacts, verifies their checksums,
and attaches those exact files and receipts to the selected release. It never rebuilds a wheel after checking it.
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
These controls and release-selection controls run before candidate builds. Full CI remains a separate gate.

## What remains outside this check

This small consumer exercises integration and packaging. The full language and operation matrix still belongs to
[analysis](analysis-coverage.md), [refactoring](refactoring-coverage.md), [proof](proof-coverage.md),
and [interface](interface-coverage.md) coverage and their hosted tests. A packaged rename does not prove every
transformation or arbitrary-program equivalence. Linux runs on each target architecture; Intel macOS execution
on an Apple silicon runner uses the runner's translation support, not independent Intel hardware.

Complete-product release qualification also requires the roadmap's outstanding feature obligations, examples,
release replay, full CI and live comparisons. Package receipts alone cannot declare that milestone complete.
