# Source repair and checked delivery

Collected in [run 38077470583](https://github.com/e6qu/fun-refactor/actions/runs/38077470583) at `10f6b4712c8b96c31e018885350c398f4f7a0427`.
The manifest binds the downloaded archive, executable and exact report bytes.
The report binds its collector, fixtures, SDK and native implementation files.

Forty cells compare repairing incorrect applied source with undo-and-correct.
Eight deliveries cover original/final checks, full reversal and independent receiver replay.
The remaining cells preserve wrong repairs, stale source/check reviews and later edits.
Missing or reordered repair patches and conflicting receiver source must refuse.

Both complete programs, their initial failed work and all fr traffic are retained.
This is a prescribed workflow comparison, without a model or an agent-efficiency claim.
It does not admit or restart any stopped client collection.

Replay with `python3 -B tools/test-source-repair-context.py`.
Full collection belongs on GitHub; local work remains under the resource guard.
