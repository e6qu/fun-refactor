# Check value tracing in real Python libraries

Suppose a user asks whether input reaches an output through a library helper. The helper’s body
can look simple while importing its package executes other code. These tests compare those
questions and keep an unfinished answer visible. See the [current library results](../../../docs/python-repository-flow.md).

## Current development direction

Toolz, funcy and pydash are already familiar test projects. Jaraco.functools remains unopened.
Keep it unread while developing the next general language changes. The
[reservation record](replacement-holdout.json) identifies the next independent check.

## Original project selection and first opening

The original [selection record](cohort.json) made Toolz and funcy development examples and
reserved pydash for the first test after freezing the analyzer. A later analyzer change requires
another unopened project. This section records that original selection; pydash is now familiar.

The initial probe calls each library's public identity helper from an authored entry module.
Source and sink hooks are identical everywhere, with no configured propagators or sanitizers.
Execute with two distinct source values, inspect the exact upstream AST independently, and
compare propagation with an overwritten value. Rename the entry and its imported helper alias,
then relocate the entry into another package. Keep the upstream implementation intact.
Keep unknown-call and exhausted-budget results incomplete, even when runtime returns normally.

Pydash first opened after coverage passed on revision `12844dee0`. That attempt stopped at
independent inspection, before native analysis or runtime execution: the harness treated typing
overloads as competing implementations. The inspector now distinguishes proven type-only
declarations and checks that an optional variadic parameter is unused by the returned expression.
The runner uses the same hash-pinned typing dependency for every project. These are harness
corrections; the native analyzer stays frozen. Retain the first opening and all subsequent results.
The project is not a fresh held-out candidate for later analyzer changes.

These are scalar probes in complete repository snapshots, not autonomous investigations or
general support for each library. The adapters are authored fixtures. A successful identity
probe alone cannot establish general repository analysis; keep B.repository-flow open until
the complete evidence and scope receive review. Preserve failed collections and every timeout.

Snapshots, runtime checks, full builds and evidence retention run on GitHub. The collector
rejects archive traversal, links, duplicate paths and declared resource-budget excess. Retain
the exact snapshot bytes, their digests, source inventories and licenses with every report.
