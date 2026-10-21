# Keep large reports outside the conversation

An agent investigating checkout may need the callers of `calculate`, without needing every function in the project.
The Python client stores the larger report and returns the requested section. It checks the stored contents before reusing them.

The Python client, or SDK, is a library that calls `fr` for another program.
A report is structured data; a packet is the selected part sent to the agent.
Materializing a section means fetching all its needed pages and assembling their values.
For a caller question, keep the full report in Python and send only that caller list.
This can reduce conversation text while still using Python memory, validation and subprocess work.

## Commands, results and limits

`ContextSession` reads the report pages needed for a selected section and keeps them in Python.
It follows commands returned by `fr`, assembles the section and checks its content fingerprint.
The agent receives only the selected fields, within the requested size limit.

```python
from fr_ir.context import DirectoryObjectStore
from fr_ir.runtime import FrClient

client = FrClient(".", executable="fr")
target = client.project("find", "render", "--signature").definition_target()
session = client.context(
    target.handle,
    view="evidence",
    token_limit=4096,
    store=DirectoryObjectStore("/tmp/fr-objects"),
)
code_map = session.materialize_section("code_map")
packet = session.packet(
    {"code_map": "/model/code_map"},
    include_actions=False,
    max_bytes=4096,
)
```

This example reads the code map for `render`. `code_map` remains a Python value until the client
sends or prints it. `packet` selects that map for the agent's answer.
If the source changes while pages are loading, start a new session from a fresh handle.

The evidence view exposes `code_map`, `call_traces`, `impact` and `sources_and_sinks` through the
same method. Semantic and project views use the same `materialize(pointer)` primitive. `packet`
accepts up to 32 named JSON paths such as `/body/0`, called RFC 6901 pointers, and returns `fr-agent-context-1` with the revision, view,
commitment, target, call count, selected values and cached object identities. It rejects a packet
outside its explicit 1–64 KiB serialized bound.

## Traversal contract

One session binds the revision, view basis, object root, target handle, view and profile from its
initial report. A followed response must carry the same identity. Traversal retains the action graph
from every response, so it can return to a disclosed ancestor and open a sibling branch. It
prioritizes an exact target action, then an ancestor action, then a continuation whose address can
reach the target. It accepts only commands returned by `fr`, detects pointer cycles during
reconstruction and enforces at most 64 follow calls for each materialized subtree.

`AgentIntent` in `fr_ir.intent` compiles a complete `understand`, `trace`, `change`, `migrate` or
`prove` request into these traversals. The intent shares a 1–512 call budget across at most eight
unique sections, emits one selected packet and checks its target, session, completeness and byte
budget before returning it. The default is 192 calls and an 8 KiB packet.

`fr intent --from` and `FrClient.compile` provide the native route for ordinary use. They construct
the same evidence model once and select the requested values without progressive subprocess calls.
`FrClient.prepare` remains the independently tested traversal route. Native results include a
content fingerprint of a report subtree (its Merkle object digest) per selection; a supplied Python object store verifies and persists those
subtrees using the same record format.

Materialization supports inline JSON values, paged objects and arrays, empty containers and UTF-8 (the text encoding used to count source bytes)
strings split at byte offsets. It requires stable node identity across pages, unique object keys,
contiguous array indices, gap-free string coverage and a final digest equal to the advertised
object digest. The returned value is detached from the retained report.

Create a directory store outside the analyzed project, or inside a path already excluded from its
snapshot. Creating cache files in the project after obtaining a handle changes the revision and
correctly makes that handle stale.

## Object storage

`MemoryObjectStore` keeps data in one process. `DirectoryObjectStore` keeps it in `objects.sqlite3`
inside the chosen directory. SQLite is a database library included with Python; no server is needed.
For example, saving an answer about `render` commits all its pieces together, instead of waiting
for thousands of separate file writes. A transaction makes either all the pieces visible or none.
Disk synchronization stays enabled, and conflicting bytes roll back the whole transaction.
Use an empty directory for a new store. The store reads only its current database format.

`store_merkle_value` uses the same `fr-merkle-object-1` shape as the CLI. It limits a pack to
65,536 objects and 64 MiB, offers the root last, reads every object back and compares canonical JSON
bytes. Canonical JSON is the normalized encoding used to compare records and compute their fingerprints.
`restore_stored_value` fetches only
reachable records, verifies every content address and reconstructs the JSON value.

The `ObjectStore` protocol requires `read()` and `put_many(records)`. A read session yields a
function that fetches a piece by its fingerprint; the local store uses one database snapshot for
the complete verification or restore. `records` maps content
fingerprints to pieces, in publication order. Local writes commit in one transaction. `HttpObjectStore`
provides a bounded zero-dependency adapter for an endpoint that accepts `GET` and idempotent `PUT`
at `<base>/<sha256>`. It accepts caller-supplied headers, refuses redirects, bounds each response to
1 MiB and exposes neither response bodies nor headers in errors. `store_merkle_value` reads every
write back and checks canonical bytes before admitting the pack. The remote service still owns
credentials, authorization, concurrency, retention, transaction support and durability.
The HTTP adapter sends one `PUT` per piece in the supplied order.

```python
from fr_ir.http_store import HttpObjectStore

store = HttpObjectStore(
    "https://objects.example/v1/fr",
    headers={"Authorization": "Bearer session-token"},
)
session = client.context(handle, view="evidence", store=store)
```

## Formal and executable evidence

`FrKernels.AgentContext` models two final admission policies. A materialization is admitted only
when its call limit is 1 through 64, the observed calls fit, every response stayed in the session,
the value is complete and its digest matches. An object pack is admitted only when it has 1 through
65,536 records, at most 64 MiB, matching digests, canonical records and a present root. Lean proves
the bounds and all required evidence. Source anchors and explicit signature maps bind both models
to their Rust predicates.

Rust and Lean agree at every selected numeric boundary and all Boolean combinations. Python and
Rust agree on the same 576-case corpus. Unit and integration tests cover pagination, Unicode byte
pages, empty arrays, duplicate/conflicting storage, session drift, pointer validation, exact call
bounds and a real `fr` traversal followed by a reviewed write. These proofs cover admission
predicates. Python execution, SHA-256 collision resistance, JSON and filesystem implementations,
the subprocess boundary and general correspondence (the stated connection between two descriptions) of the traversal algorithm remain trusted or
integration-tested components.

`FrKernels.AgentIntent` models the complete intent admission conjunction. It proves that every
accepted request satisfies all projection, section, call and packet bounds and that target,
session and completeness evidence are all true. Rust and Lean agree on 32,768 boundary cases;
Python and Rust agree on the same corpus. A real three-section trace test exercises the Python
compiler against `fr`, including sibling traversal and content-addressed storage.

`FrKernels.AgentIntent.actionMode` models intent-bound change admission. Preview requires the
`change` purpose, a complete action and no write or basis. Execution requires the same purpose and
action plus write intent and an exact supplied basis. Lean proves both characterizations, and Rust
agrees over all 112 public-purpose and Boolean cases. Task planning, source verification, checks,
history and filesystems remain integration-tested boundaries.

## Controlled comparison

The baseline sends every requested page and reply through the agent's conversation.
The compared approach fetches the same pages inside Python, then returns only the selected section.
Both do the same internal page work. The byte difference includes the complete Python program and result.
Python memory, validation and elapsed execution overhead are unmeasured in this comparison.

The [current Rust example](../tests/agent-eval/results/2026-10-21-final-ci-agent-context-workspace/result.json)
constructs the same map of files and functions through 24 internal `fr` calls in both approaches.
Sending every request and reply to the agent costs 89,677 bytes across 24 exchanges.
The complete program request costs 695 bytes, including its code and invocation.
Adding the 4,085-byte selected result makes 4,780 bytes in one exchange, a 94.7% reduction.
Both approaches retain the same map and saved objects.

```sh
python3 tools/agent-context-workspace.py --fr target/debug/fr \
  --output /tmp/fr-context-workspace.json
python3 tools/agent-context-workspace.py \
  --audit /tmp/fr-context-workspace.json
```

This fixed protocol measurement does not run a model or measure tokens, hidden reasoning, billed
quota or results across a wider range of tasks.
