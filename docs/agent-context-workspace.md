# Agent context workspace

`ContextSession` turns `fr` progressive disclosure into a bounded local workspace. An agent names
the high-level section it needs; the runtime follows only exact server-issued actions, reconstructs
that subtree, verifies its Merkle digest and returns a selected packet. Intermediate reports remain
inside the Python process.

```python
from fr_ir import DirectoryObjectStore, FrClient

client = FrClient(".", executable="fr")
handle = client.project("find", "render", "--signature").at("/rows/0/0")
session = client.context(
    handle,
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

The evidence view exposes `code_map`, `call_traces`, `impact` and `sources_and_sinks` through the
same method. Semantic and project views use the same `materialize(pointer)` primitive. `packet`
accepts up to 32 named RFC 6901 pointers and returns `fr-agent-context-1` with the revision, view,
commitment, target, call count, selected values and cached object identities. It rejects a packet
outside its explicit 1–64 KiB serialized bound.

## Traversal contract

One session binds the revision, view basis, object root, target handle, view and profile from its
initial report. A followed response must carry the same identity. Traversal prioritizes an exact
target action, then an ancestor action, then the current page continuation. It accepts only commands
returned by `fr`, detects pointer cycles during reconstruction and enforces one shared limit of at
most 64 follow calls.

Materialization supports inline JSON values, paged objects and arrays, empty containers and UTF-8
strings split at byte offsets. It requires stable node identity across pages, unique object keys,
contiguous array indices, gap-free string coverage and a final digest equal to the advertised
object digest. The returned value is detached from the retained report.

Create a directory store outside the analyzed project, or inside a path already excluded from its
snapshot. Creating cache files in the project after obtaining a handle changes the revision and
correctly makes that handle stale.

## Object storage

`MemoryObjectStore` is process-local. `DirectoryObjectStore` shards canonical JSON records by their
lowercase SHA-256 digest and publishes immutable files atomically. `store_merkle_value` uses the
same `fr-merkle-object-1` shape as the CLI. It limits a pack to 65,536 objects and 64 MiB, writes the
root last, reads every object back and compares canonical bytes. `restore_stored_value` fetches only
reachable records, verifies every content address and reconstructs the JSON value.

The `ObjectStore` protocol has only `get(digest)` and `put(digest, record)`, so an adapter can use
object storage. The SDK verifies an adapter's writes but does not provide credentials, transport,
concurrency control, retention or remote durability guarantees.

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
the subprocess boundary and general correspondence of the traversal algorithm remain trusted or
integration-tested components.

## Controlled comparison

The retained generic fixture materializes the same code map in both arms with 16 internal `fr`
calls. Exposing every progressive request and response costs 59,825 bytes across 16 agent-visible
exchanges. The complete 652-byte Python program request and its 3,864-byte selected packet cost 4,516 bytes
in one exchange, a 92.5% reduction. Both arms retain the same normalized code map, call count,
packet size and cached-object count.

```sh
python3 tools/agent-context-workspace.py --fr target/debug/fr
python3 tools/agent-context-workspace.py \
  --audit tests/agent-eval/agent-context-workspace.json
```

This fixed protocol measurement does not run a model or measure tokens, hidden reasoning, billed
quota or population behavior.
