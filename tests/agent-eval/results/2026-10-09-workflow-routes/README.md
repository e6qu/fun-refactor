# Checked editing routes

See [the workflow inventory and results](../../../../docs/workflow-routes.md) for the comparison
design, measurements, refusal controls and limits.

`manifest.json` binds the two reports to GitHub run 37973505951, attempt 2, its source commit,
binary and original artifact digest. `result.json` retains 15 successful edit cells and 23 refusal
cells with their requests and responses. `agent-guide-context.json` is the separate complete-program
scalar comparison. The compatibility copy remains at `tests/agent-eval/agent-guide-context.json`.

Replay without launching fr or a model:

```sh
python3 -B tools/test-workflow-route-context.py
```

The sources must match the reports' bound revisions. Full collection is restricted to GitHub;
the `workflow-routes` group of `refinement-evidence.yml` has a 15-minute execution ceiling.
