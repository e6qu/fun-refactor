# Does continuing saved work cost less?

You have already found a function and its callers. After a pause, you need the current answer again.
Should you reopen that work, or ask the same questions from scratch?

For example, Boltons provides `clamp`, which limits a number to a chosen range.
The comparison first finds its declarations and the callers that `fr` can identify.
It repeats the question without an edit, then changes one source file and asks again.

## The comparison

Both approaches use the same optimized `fr` program and Python SDK.
The SDK lets a Python program ask `fr` questions and read its replies.
Both approaches retain the native project cache between questions.
That cache contains work that `fr` can reuse when it reads a project again.
Each approach starts with an empty, separate cache inside its workspace.
The cache path is explicit. One approach cannot populate the other approach's cache.

The fresh approach repeats discovery. It keeps no previous answer or saved task.
The saved approach stores the answer and its task plan.
The plan records which files, name lookup and analyzer implementation the answer depends on.
Before reusing the answer, `fr` checks those inputs.
When they change, the saved approach repeats discovery too.

The source edit replaces the selected name in one declared file through `fr file replace`.
For example, the edit changes occurrences of `clamp` to `clamp_cost_renamed` in `boltons/mathutils.py`.
Both approaches then ask about the original name, `clamp`.
The question tests whether the saved answer expires. It does not test a complete project rename.

## What counts

Every question starts a new Python process. Its measured time includes these costs:

- Starting Python and importing the SDK.
- Reading and checking saved inputs, when present.
- Finding declarations and inspecting their bounded callers.
- Storing new answers and writing the response.

The record also counts CPU used by the process and its children.
It samples their combined memory and records disk growth.
Samples can miss short memory peaks. GNU time reports process-tree CPU in hundredths of a second.
The worker also records CPU with finer precision before writing its final report.

Source download, extraction, checkout copies and the source edit have separate measurements.
The three-question session total includes its checkout copy, edit and cleanup.
Download and extraction are shared preparation, outside each session total.
The optimized build and runner environment setup also stay outside request totals.
The workflow records their steps and duration in GitHub.

Storage includes the native project state and saved task objects.
The native state includes the project cache and the edit history.
The report counts the complete submitted Python program, invocation, request and returned answer separately.
Intermediate native reports use canonical JSON byte counts. They exclude formatting differences in command output.

## Projects and independence

The development projects are Rust glob and Python Boltons.
The independent project is Python JMESPath, which searches JSON documents.
Its public README shows `jmespath.search('foo.bar', {'foo': {'bar': 'baz'}})` returning `'baz'`.
Its implementation and tests stay unread until the procedure, source identities and optimized binary are frozen.

Each project has three repetitions of first use, unchanged reuse and post-edit analysis.
The order of the two approaches alternates between repetitions.
Operating-system file caches remain uncontrolled. Three repetitions cannot establish a broad speed claim.

Every saved answer must match fresh discovery on the same files.
Unresolved names, ambiguity and limits on checked callers remain visible in that answer.
A passing comparison establishes agreement within those limits.

The workflow retains failed attempts. A failed first independent evaluation remains the first result.
After inspecting that implementation to fix support, a new independent project must supply any new first-evaluation claim.

## Scope of the result

This comparison measures scripted questions about code.
It does not measure an agent discovering a repair or finishing an unfamiliar task.
It also does not compare Python execution with another language.
The [benchmark guide](benchmarks.md) explains those separate questions and their required baselines.

The [declared tasks](../tests/agent-eval/saved-work-cost/tasks.json) fix the questions and limits before collection.
GitHub runs the [measurement workflow](../.github/workflows/saved-work-cost.yml).
The retained results will show whether saved validation avoids enough repeated work to cover its own costs.
