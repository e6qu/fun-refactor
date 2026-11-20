# Refresh evidence after changing behavior

Freeze the implementation and its tests before collecting reports. Reports often bind the entire
native source tree; some also bind shared test files, Python client code or collector workflows.
Changing one of those files after collection can invalidate several otherwise successful reports.

Run the **Evidence preflight** workflow on the working branch. It lists every stale report selected
by the current scalar manifest, active acceptance tests and current roadmap records, with the changed
inputs for each. It reads report JSON from the sparse checkout's Git objects without extracting archives.
The JSON artifact preserves the complete list even when the job fails.

Use the existing **Refresh refinement evidence** groups to collect the affected families on GitHub.
Keep the complete scalar family together when its common inputs change. Retain successful collections
through their existing retention jobs, then refresh project replay and session-cost records as required.
Run publishing jobs sequentially so branch updates do not race. Run preflight again before full CI.

CI requires this preflight before native tests, static checks and current project/session/package replays.
Focused graph and analysis checks can run while reports are stale, so implementation failures remain
easy to diagnose before collection. A green preflight establishes recorded input freshness only:
the existing behavior, corruption, archive and replay checks still have to pass.

Historical studies retain their original sources and outcomes. They do not need recollection merely
because current code changes. Reports without source bindings are labelled `not-source-bound`; the
preflight does not turn them into current evidence. Adding a new current report family requires adding
its selector and a test here as well as its existing behavior gate.

On an admitted workstation the command is `python3 -B tools/evidence-preflight.py` through the local guard.
When the guard refuses, use GitHub; do not raise resource limits or run the check outside it.
