# OpenCode memory admission rejected

[GitHub run 37496922352](https://github.com/e6qu/fun-refactor/actions/runs/37496922352)
measured the pinned OpenCode 1.18.34 client with default Bun settings and
`BUN_OPTIONS=--smol`. Each platform ran three samples of each mode, serially,
against a scripted loopback provider. No live model or provider credentials were used.

| Platform | Default peak RSS, MiB | `--smol` peak RSS, MiB | Capture outcome |
| --- | --- | --- | --- |
| Linux x64 | 690.86, 691.98, 685.89 | 689.91, 683.25, 680.55 | All six completed |
| macOS arm64 | 771.44, 774.19, 781.19 | 776.75, 777.08, 785.06 | All six stopped at the RSS cap |

Both platforms reject admission. Every lower-memory sample must complete below
640 MiB, leaving 128 MiB below the unchanged 768 MiB capture cap. Sampling can
observe an overshoot before killing the process group. The controls do not
identify which process caused the aggregate peak.

The proposed runtime setting did not establish enough headroom. Do not use this
result to start a workstation control or fresh Kimi/GLM review. The packaging
pilot remains blocked, and the earlier live collection remains stopped.

The archives retain all downloaded artifact files, including partial failures,
plans, process measurements and successful source-delivery audits. The manifest
binds their bytes and the original checker. `report.json` is independently
replayed by `tools/test-client-memory.py`; the frozen checker preserves the
meaning of this historical result after the CLI gains separate measurement and
admission commands. Neither historical failures nor limits are rewritten.

The original workflow failed because it required a favorable experimental result.
The revised workflow checks that measurement and evidence validation succeeded,
then publishes the separate admission decision. `check` and `admit` still exit
unsuccessfully when headroom is insufficient. A passing diagnostic job does not
mean the client is admitted or the code-change pilot has run.

The next resource investigation needs process and capture-phase measurements on
GitHub to locate the peak before choosing another remedy. Do not raise limits,
repeat live attempts, or conclude that `--smol` fixes the memory failure.
