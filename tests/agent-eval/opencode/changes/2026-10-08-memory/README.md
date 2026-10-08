# Change-adapter memory admission remains blocked

GitHub run [37680442901](https://github.com/e6qu/fun-refactor/actions/runs/37680442901)
tested the configured change adapter at commit `57f63836`. Linux completed all five
scripted controls within the existing 640 MiB admission threshold. macOS completed
the ordinary Kimi control at 571.14 MiB, then the fr control at 739.92 MiB. The fr
control stayed below the 768 MiB capture limit but failed admission. Three remaining
macOS cells did not start. No live model call ran.

The two ZIP files retain the original GitHub artifacts. `provenance.json` binds
their archive digests, artifact IDs, run and source commit. The archives include
the frozen inputs, transcripts, exact submissions, process records and reports.
`tools/test-change-memory.py` replays them offline and preserves failed admission.

The next hosted measurement adds process attribution using the same samples as
the resource guard. It keeps both memory thresholds and all other limits unchanged.
Do not infer workstation admission from Linux results or completed submissions.

The proposed remedy freezes `BUN_OPTIONS=--smol` in new change plans alongside
the small catalog. Bun documents this as a lower-memory mode with more frequent
[garbage collection](https://bun.sh/docs/runtime). Earlier full-catalog captures
did not pass admission with this flag; the bounded change adapter needs its own
hosted measurement. Old plans retain their original empty option. This is a
hypothesis until both platform controls pass; no resource limit increases.
