# Whole packaging review with explicit low reasoning effort

This fresh two-model collection keeps the complete question, eight-case grader
and source snapshots from `2026-10-06-packaging-inputs`. Both profiles now select
`low` reasoning effort with the same 2,048-token output allowance. Earlier
failed collections remain permanently stopped.

The plan binds the design, collector, provider catalog and hosted/workstation
admission evidence. All eight hosted transport controls and the single fresh
workstation control completed below 640 MiB. The collector requires a committed
plan and inputs before any model call. OpenCode uses its existing configured
authentication; the collector does not inspect or copy credentials.

Capture limits remain 120 seconds, 20 CPU seconds, 768 MiB sampled aggregate
RSS, 16 MiB disk growth and 1 MiB transcript. Local calls also use the outer
resource guard. The first resource-limit failure permanently stops collection.
Candidate code cannot execute during review. Model findings require independent
verification; empty findings do not imply whole-task acceptance.

The four ordinary-tools/fr code-change pilot attempts remain unstarted.
