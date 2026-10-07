# Explicit reasoning effort reaches both provider adapters

[GitHub run 37591473150](https://github.com/e6qu/fun-refactor/actions/runs/37591473150)
passed on Linux and macOS at commit `3e6a59f4`. Each platform ran four serial
model-free controls: Kimi K3 and GLM 5.3 Flash, each at `low` and `max` effort.
The pinned OpenCode client sent the selected `reasoning_effort` and an unchanged
`max_tokens: 2048` on both requests in every control. Each source read reached
the second request, and each structured submission completed.

| Platform | Control peaks, in matrix order (MiB) |
| --- | --- |
| Linux | 474.14, 473.95, 468.41, 479.46 |
| macOS | 549.41, 579.94, 562.22, 554.69 |
| Workstation, Kimi low only | 545.30 |

The six-run memory admission experiment also passed separately on both hosted
platforms. One fresh guarded workstation control passed after the corrected
validator reached GitHub. It used 2.06 CPU seconds, 4.99 elapsed seconds and
about 6 MiB of disk growth. No limit was raised.

The archives retain uploaded hosted evidence and the complete cleaned local
capture. The manifest binds all archives and the exact control implementations.
`tools/test-review-reasoning.py` replays the requests, messages, source delivery,
submission, resource records and memory decisions without starting clients.
The earlier failed validator capture remains in `../2026-10-07-variant-shape`.

These controls establish client behavior. They do not establish that either
remote model honors the requested effort, completes a live review, or uses fr
effectively. Those questions require a separately frozen live collection.
