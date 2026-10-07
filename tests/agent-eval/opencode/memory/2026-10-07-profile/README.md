# OpenCode dominates the measured capture peak

[GitHub run 37566726066](https://github.com/e6qu/fun-refactor/actions/runs/37566726066)
profiled three default and three `--smol` captures on each platform. Both diagnostic
jobs passed: Linux took 66 seconds and macOS took 38 seconds. Admission remains
rejected on both platforms.

| Platform | Aggregate peak range | OpenCode RSS at those peaks | Outcome |
| --- | --- | --- | --- |
| Linux x64 | 680–700 MiB | 621–641 MiB | Six captures completed above the 640 MiB target |
| macOS arm64 | 774–788 MiB | 658–673 MiB | Six captures stopped at the 768 MiB cap |

Every peak sample falls within the inferred message-in-flight stage. OpenCode is
the largest process in all twelve samples. Two Python processes account for the
remaining measured RSS; no `fr` process appears at these peaks. On macOS, the
client alone exceeds the entire 640 MiB admission target. Removing capture-host
memory alone therefore cannot meet that target in these samples.

This attributes measured process RSS, not allocations within OpenCode. Sampled
limits can overshoot before stopping a process, miss brief peaks, and count shared
pages more than once. A process absent at the peak may have run at another time.
The measurements do not establish the cause of all previous workstation problems.

Both archives retain every downloaded artifact, including failed captures and
bounded process traces. The manifest binds their bytes and the three implementation
files. `tools/test-client-memory-profile.py` replays them using the frozen checker
and profiler. Per-process sums reproduce the guard's aggregate peak and CPU totals.
Historical results remain unchanged when a new experiment is prepared.

The next hypothesis concerns catalog initialization. The pinned client's
[catalog loader](https://github.com/anomalyco/opencode/blob/v1.18.34/packages/core/src/models-dev.ts)
uses a file selected by `OPENCODE_MODELS_PATH` before falling back to its compiled
snapshot. Disabling model fetching alone does not disable that snapshot.
Its [catalog plugin](https://github.com/anomalyco/opencode/blob/v1.18.34/packages/core/src/plugin/models-dev.ts)
transforms the loaded providers and models.

A separate hosted experiment compares the default snapshot with an empty catalog
for the explicitly configured scripted model. All source-delivery and terminal-answer
checks still apply. The catalog is hash-bound and passed only to the child process;
default controls clear that option. This is a hypothesis, not a demonstrated fix.
It does not establish provider metadata or authentication behavior for Kimi or GLM.
