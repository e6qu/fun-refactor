# Check an explanation against the code the agent read

Suppose an agent explains when a cache entry expires. Check its answer against the source it inspected, including the helper that decides expiry.
The comparison below records those source reads and grades the answer separately from how many tool calls the agent made.

A source citation points to code supporting a factual claim. A rubric lists the claims the grader checks.
Disclosure counts the source or results delivered to the agent, not everything the computer read internally.
For “when does this retry?”, a correct answer must explain the relevant condition and cite the code the agent received.
Compare ordinary file tools with the same tools plus `fr`; text counts alone do not measure correctness or billing.

## Questions, source reads and recorded answers

The OpenCode explanation comparison asks four factual questions in each of three pinned Python
repositories: cachetools expiration, itsdangerous key rotation, and tenacity retry callbacks.
Each repository is a complete upstream snapshot with its license. The tenacity README symlink
is materialized as its target's text; runtime source is unchanged. Archive hashes and this
transformation are retained in [the source inventory](../tests/agent-eval/opencode/repositories/README.md).

Both tool setups offer file listing, literal search and bounded source reads. The fr setup also
offers public `project map`, `find` and `show` operations. Agents choose their tools. Search results
include byte offsets and content hashes for direct reads; fr discovery supports file filters and
opaque continuation cursors, and `show` supports source continuation offsets. These are generic
host capabilities: neither the host nor fr contains rules for the three repositories.

The [manifest](../tests/agent-eval/opencode/explanations.json) selects the configured Kimi K3 and
GLM 5.3 Flash profiles, with one attempt per model, task and tool setup: twelve attempts. It uses
the [same resource guard and subprocess budgets](opencode-rehearsal.md) as the synthetic rehearsal.
Freeze `explanations.json` instead of `manifest.json`, then run the frozen cells in order. There are
no retries or larger budgets for failed attempts. CI makes no model calls.

## What a passing answer establishes

The agent returns an object with a value and exact source quotations for each requested fact.
A private rubric fixes the expected values and operative source statements before the calls.
The grader requires every quotation to match the pinned source and to have appeared in a tool
result delivered to that agent. An accurate guess without retrieved evidence fails. Explanations
are read-only; an edit request refuses. The grader never imports or executes repository code.

For the cache example, an answer needs the source statement that decides expiration, not just the
class declaration. A correct guess can still fail this source-reading task if the agent never read
the required statement. That score concerns this fixed question; it cannot certify general understanding.

This is a finite factual check. It does not judge arbitrary prose, prove a theorem, or establish
that the agent understands the entire program. Required source anchors can reject a correct
explanation expressed with different evidence. CI separately runs reviewed behavioral controls
against the pinned libraries, including boundary times, key order and retry hook order. Those
controls check that the factual rubric agrees with execution; they do not expand the model's score.

Two repositories were used while implementing the adapter. The third rubric was written after
the first implementation commit as a subsequent integration check. All rubrics were reviewed by
the developer. This is neither a blinded study nor a claim of independence from model training.

## What the retained measurements establish

Protocol 2 replays ordinary host actions and checks retained fr stdout against delivered results.
Every source snippet must match the frozen file's exact bytes. Overlapping read, search and fr
source ranges are deduplicated by file path and content hash. Reports include exact, unique and
repeated source bytes, instruction bytes, tool-result bytes, refused calls and unfinished calls.
Produced tool-result bytes remain separate from bytes carried into a subsequent model prompt.
A result produced after the final allowed turn contributes no disclosed source bytes.
Names, signatures and other metadata remain in tool-result totals but are not counted as exact
source pages. Source bytes and model tokens are different measurements.

The report preserves the CLI's input, output, reasoning and cache counters. Complete provider
context, verified billing, global OpenCode storage, escaped processes and full-system costs remain
unknown. A partial source-byte counter does not establish total context savings. All failures stay
in the denominator, including attempts that time out before returning a complete response.

The frozen plan holds each source bundle once; attempt records refer to its hash. Raw model/tool
streams, session exports when available, answers, grades, timing and sampled process memory remain
available for offline replay. The six frozen runner modules and grader are retained with their
hashes. Historical protocol 1 results retain their original audit format.

The [twelve retained attempts](../tests/agent-eval/opencode/results/2026-10-01-explanations/README.md)
include one pass, one completed answer rejected for its schema and citations, and ten incomplete attempts. Two fr attempts
used CLI discovery. These outcomes do not support an efficiency claim. The original report counted
results produced after the final allowed turn as disclosed. The corrected report preserves the
original outcomes and excludes those results from source-disclosure totals; both reports remain.

One diagnostic found that OpenCode 1.18.34 truncated a session export at 65,536 bytes through a pipe.
The same session exported 66,953 valid JSON bytes to a regular file. Future runs capture exports
through a temporary regular file with a sampled 1 MiB size limit, then forward the bytes
to the existing transcript collector. The original comparison keeps its frozen implementation and
the export failure. Re-exporting a session does not turn that failed attempt into a passing one.
The monitor places the temporary file inside the attempt's existing disk budget. A process-wide
file limit also blocked OpenCode's existing SQLite checkpoint, so the export cap applies only to
the export file. The diagnostic refusal and the successful corrected capture remain available.

Explanation grading is one step toward the original goal. Repeatable unfamiliar-project changes,
proofs connected to the actual source, useful delegation and complete cost/context measurements remain open.
The [current status](roadmap-status.md) owns the acceptance counts. These explanation attempts do not establish a general benefit from fr.
