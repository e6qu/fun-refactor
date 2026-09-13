# Bounded agent discovery evaluation

PR 20 addresses the failure recorded by the PR 19 agent trace: an agent found the right declarations
but spent 239,321 input tokens after launching cold queries in parallel, widening limits and reading
broad project output. This evaluation separates the enforced protocol, cache coordination and
formal model from claims about agent behavior.

## Discovery protocol

`project explore TERM` starts in names mode. Compact mode returns at most twelve declarations and no
source. Each row contains a full revision-bound handle and an exact argument array for behavior mode.
Behavior mode accepts only a matching full handle in the selected scope. It returns at most 2,048
source bytes and eight direct incoming or outgoing relationships. Source and relationship pages
carry exact continuation argument arrays.

`--profile expanded` is an explicit mode change recorded in the response. Its ceilings are thirty
name rows, twenty relationships, 4,096 source bytes and a 32,768-byte report. Both profiles reject
an output that exceeds the selected report ceiling.

A `project batch --profile compact` manifest admits at most eight `explore` requests and clamps the
shared nested-report budget to 16,384 bytes. Expanded batches admit sixteen requests and 32,768
bytes. Profiled batches reject every other project subcommand. A later behavior request can consume
`/rows/0/handle` from an earlier names result, so one process constructs and verifies one project
view for both stages. The `frpqb2:` manifest basis includes the outer profile.

## Concurrent resolution

Resolution snapshots have one cache-keyed owner lease. Another process with identical extracted
resolution inputs polls for the completed snapshot. Publication uses the existing temporary-file
write and integrity envelope. A waiter admits only the complete checksummed snapshot with the exact
reference length and bounded target identities.

The wait is bounded at 310 seconds. A lease older than 300 seconds is recoverable, so a killed owner
cannot block the key indefinitely. After creating a lease, a candidate owner rechecks for a snapshot
published between its earlier read and lock acquisition. This closes the race that could otherwise
perform a second build after the first owner had already finished. Waiters emit monotonic
`resolution-wait` progress in JSON mode.

The filesystem provides creation exclusivity and timestamps. The model does not prove clock
behavior, process liveness, scheduler fairness or hostile concurrent replacement. A timeout falls
back to an uncached local resolution rather than accepting uncertain shared state.

## Generic fixture

[`tools/agent-discovery.py`](../tools/agent-discovery.py) creates a generic Python project with a
dependency, one selected behavior and one caller. It launches two cold `project explore` processes
against one cache, follows the bounded behavior route, runs a referenced profiled batch and checks
stale-handle refusal after an edit.

The retained run in
[`tests/agent-eval/agent-discovery.json`](../tests/agent-eval/agent-discovery.json) completed both
cold processes in 1.952358 seconds on one host. They returned byte-identical reports, recorded one
resolution owner, one waiter and no timeout. The compact behavior response returned 46 source bytes,
two relationship rows and 1,890 serialized bytes. The two-stage batch reported one project view and
the enforced 16,384-byte aggregate limit. These are correctness and diagnostic measurements, with
no production latency threshold.

```sh
python3 tools/agent-discovery.py --fr target/debug/fr
python3 tools/agent-discovery.py --audit tests/agent-eval/agent-discovery.json
```

## Repository dogfood

Before coalescing, two overlapping exact lookups both entered resolution for 437,220 references and
each ran for about a minute. After coalescing, two cold `project explore batch_manifest` calls over
961 files and 438,948 references recorded exactly one owner and one waiter. Only the owner emitted
resolution progress. The waiter emitted `resolution-wait` progress through 67 seconds, consumed the
published snapshot and completed with the same report bytes and revision. Neither process timed out.

The first behavior-stage implementation built the complete 47,236-edge call graph to inspect a
236-byte function and took 22.189 seconds. Dogfooding exposed that mismatch before review. The route
now uses direct indexed relationships, retaining direction, confidence, resolved targets and
pagination without graph construction. Full dispatch expansion remains available through the
separate `project calls` command.

The final stable-tree dogfood indexed 963 files and 439,439 references. Names mode returned two
matches in 1,675 serialized bytes. Following the selected argument array returned the 236-byte
declaration, eight direct relationships and its continuations in 3,203 serialized bytes and 1.713
seconds on this host. The equivalent two-stage compact batch completed in 1.700 seconds, reported
one project view and used 3,520 of its 16,384 nested-report bytes. These debug-build measurements are
diagnostics, not latency thresholds.

No new autonomous Codex run is counted as evidence here. The earlier failed and interrupted Luna-low
traces remain immutable evidence of the behavior this protocol addresses. A future authenticated
trial must use the portable skill without extra route instructions before it can support a fresh
agent-efficiency claim.

## Formal coverage

`FrKernels.AgentDiscovery` proves that accepted discovery responses stay within the selected row,
source and report ceilings. It proves the target requirement for behavior mode, stale-owner recovery
priority, bounded-wait termination selection and owner-only admitted publication. Four strict source
anchors connect those executable decisions to Rust. The shared suite checks 3,472 Rust/Lean cases.

The model covers the decision kernels. Host tests cover independent cache handles, atomic snapshot
visibility, stale recovery, timeout, invalid-snapshot replacement, byte-identical indexes, profiled
batch references, exact continuations and stale handles. Parsing, JSON and postcard serialization,
SHA-256 collision resistance, filesystem semantics and process scheduling remain in the trusted or
tested boundary.
