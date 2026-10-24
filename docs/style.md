# Write for someone joining the project today

Begin with the reader's task. Explain what they can do, show an example and say how to check the result.
Keep command names and data fields precise. Explain their meaning where the reader first needs them.

For example, introduce a transaction as a saved change with an ID before asking someone to run `fr history undo '<TX>' --write`.
Explain where that ID comes from. A glossary link can add detail, but it cannot supply a missing step in a lesson.

## Choose the right kind of page

| Page | What belongs there | Example |
| --- | --- | --- |
| Tutorial | Setup, complete starting files, steps, expected results and a useful failure | Create a Python helper, rename it, inspect the diff and undo the change. |
| Reference | Exact commands, fields, limits and refusal behavior | State whether a failed check leaves the edited file in place. |
| Roadmap | Named next changes and observable completion checks | Trace input into an error message after a failed integer conversion. |
| Bug list | A reproducible wrong result and its current status | A trace claims completion after overlooking an import. |
| Measurement guide | Baseline, measured operation, setup, units and limits | Compare saved Python setup with fresh setup using the same native binary and questions. |
| Historical report | What happened in that attempt, including failures | Preserve a timed-out trial and count its consumed work. |

Keep priorities in [PLAN.md](../PLAN.md), defects in [BUGS.md](../BUGS.md) and operational instructions in the [handoff](continuity.md).
Link to the owner of a fact instead of copying a second backlog or cost table.
Remove closed work from active lists. Preserve its tests, original reports and an immutable history link.

## Teach the example before the notation

State the user's problem before presenting code. Name the file to create and the folder from which to run a command.
List required tools and setup. Say whether a snippet runs alone, continues an earlier example or only illustrates a concept.

Define placeholders and returned IDs before asking the reader to reuse them.
For example, `/body/0` selects the first item in a function's `body` list. Introduce that selection before showing a large JSON edit.
Explain a parameter as a named function slot before describing Python's positional-only syntax.

After the example, say what the result establishes and what the reader should do next.
A passing build does not establish that a reported rounding error is fixed. Name the behavior test needed to check that claim.
If a failure leaves files edited, explain how to inspect or undo the saved change before retrying.

## Explain technical details through their consequences

| Unclear wording | Concrete explanation |
| --- | --- |
| “Retained basis admission failed.” | “Checkout changed after your preview. Prepare and review a new diff.” |
| “The flow is incomplete.” | “The import search stopped, so an empty trace cannot rule out the connection.” |
| “The receiver replay passed.” | “The patch applied in a separate checkout, and its declared tests passed there.” |
| “The proof is discharged.” | “Lean accepted the stated model. Its connection to the shipped function still needs checking.” |
| “The source identity changed.” | “The report records the earlier helper. Recheck it after changing that helper.” |

Keep exact fields such as `complete: false` beside these explanations when readers need to recognize tool output.
Do not replace required terminology with invented synonyms. Define it through a small example, then use it consistently.

## Write a useful plan or PR description

Name the concrete problem and resulting behavior first. Show a before-and-after example when it helps.
For a planned change, state its initial scope, unsupported cases, relevant checks and conditions for completion.
“Improve Python support” is too broad. “Follow a helper's write to a shared list and forget overwritten values” gives a testable goal.

A PR description should explain the result to someone who did not follow the conversation.
Include remaining limits and the checks that ran. Link detailed test inventories and run history instead of repeating them.
For an older PR, describe what it established at the time; later support belongs in the current guide.

## Explain a performance comparison

Name both sides and the measured operation before giving a percentage.
Report raw amounts, units, repetitions, cache state, included preparation and validation, and unknown costs.
Explain added work: parsing a project, starting a Python process or sending instructions to an agent all have costs.

Keep these questions separate:

- How fast does the generated program run compared with the original program?
- How much time and memory does `fr` use to answer the same question as another tool?
- How much text reaches the agent, including setup and later requests?
- What does the complete agent task cost, including retries and failed attempts?

A smaller report cannot establish a cheaper complete task. A model proof cannot establish runtime agreement on its own.
The [measurement guide](benchmarks.md) owns the current comparisons and their limits.
Update generated descriptions in their renderer so regeneration preserves the explanation.
Preserve original trial outputs, failed attempts and recorded hashes; collect fresh results when their inputs change.

## Keep the language direct

Use familiar words, active verbs and one topic per paragraph. Prefer “use” to “utilise” and “before” to “prior to”.
Say who acts when it matters: “The tool refuses the rename” tells the reader more than “The rename is disallowed”.
Use present tense for current behavior and past tense for dated observations.
Remove filler, repeated conclusions and decorative contrasts. Let a short sentence carry a fact.

Aim for sentences of at most 25 words. The writing check tracks long sentences and other habits in maintained manuals.
`tools/check-prose.py` runs from `tools/check.sh`; `tools/PROSE-DEBT` records the allowed counts.
Improve the writing when a check fails. Do not raise those counts to pass.
Passing this check does not establish that the page teaches its subject clearly.

Comments should explain a reason, constraint or unit that the code cannot make clear.
For example, `// Bytes: tree-sitter reports byte offsets.` explains a unit that matters to edits.
Avoid comments that repeat the next statement or recount a closed defect. Keep that history in tests and Git.

## Review the whole page

Read the detailed sections as a newcomer, including tables and examples.
Check links, first uses of concepts, command syntax, setup and interpretation of output.
Keep one current report format. Do not imply that a client accepts earlier formats or fills missing metadata.
Treat an unknown answer, a negative answer, a preview and a tested repair as different outcomes.

The [hosted documentation review](development.md#review-documentation-changes-on-github) checks links and refreshes generated pages.
Use it after editing the authoritative guides and before requesting final CI.
