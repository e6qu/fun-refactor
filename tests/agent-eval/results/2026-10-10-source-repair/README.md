# Source repair and checked delivery

Collected in [run 38055066754](https://github.com/e6qu/fun-refactor/actions/runs/38055066754) at `c22dfb0f25b2c4eb0a73d598ca7566b19b0dc643`.
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
