# Restore historical evidence without changing its contents

Eleven historical flow-cache reports now use gzip storage. The
[catalog](evidence-archives.json) records each original path, byte count, SHA-256 and Git blob ID,
plus the compressed file's identity. It names commit `ced10cb8` as the source of the original files.
The conversion checked every restored byte against that commit's blob identity before removing
the expanded copy. It changed no samples, failures, measurements, source bindings or conclusions.

The current acceptance report remains at
[`2026-09-30-assignments-flow-cache/result.json`](results/2026-09-30-assignments-flow-cache/result.json).
Its evaluator, roadmap entry and source-bound checks remain unchanged. Historical reports retain
their historical bindings; restoring one does not make it current or establish a new result.

## Size and scope

| Tracked content | Before | After |
| --- | ---: | ---: |
| Eleven historical reports | 409,344,894 bytes | 24,891,122 bytes |
| Complete retained-results tree | 656,952,323 bytes | 272,498,551 bytes |

This removes 384,453,772 bytes from an expanded checkout, about 366.6 MiB. Existing Git history still
contains the originals. This does not shrink history, build caches or prove lower agent token use.
An inventory found only 4.7 MB of exact duplicate-file content across the whole results tree;
simple file deduplication would save much less than compressing these large historical JSON reports.

## Verify and restore

Run these commands from the repository root. On the shared workstation, prefix them with
`python3 /Users/zardoz/.codex/tools/fr-local-guard.py`.

```sh
python3 tools/evidence-archive.py check
python3 tools/evidence-archive.py restore \
  tests/agent-eval/results/2026-09-25-flow-cache/result.json \
  /tmp/fr-2026-09-25-flow-cache.json
```

The first command verifies all registered archives using bounded streaming reads. The second writes
one exact original file to a new destination. It refuses existing files and removes partial output
on validation failure. Reading an archive never executes its contents. Standard gzip readers can
also open these files; the helper additionally checks catalog hashes and original Git blob IDs.

Each file allows at most 8 MiB compressed and 64 MiB restored. The catalog permits at most 64 files
and 1 GiB of restored data. Verification reads one file at a time in 128 KiB chunks. CI verifies all
archives and tests corrupt data, oversized output, invalid paths, symlinks and failed restoration.
Restore only the report needed for an investigation; the active audit needs no restoration step.

The [original September 25 report](results/2026-09-25-flow-cache/result.json.gz) supports the historical
cache comparison. The other ten archives preserve intermediate reports from September 28 and 29.
All remain available in a fresh checkout without a separate download or external artifact service.
