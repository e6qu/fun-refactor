# How to write here

Write for someone who has not followed the project's development.
They need to understand the task, the words used to explain it, the action to take and the result to expect.
For example, a reader should understand how to review a rename without already knowing what a handle or transaction is.

The sentence rules draw on Simplified Technical English, used to make technical procedures easier to follow.
Passing the automated writing check does not establish that a page teaches its subject clearly.

`tools/check-prose.py` counts the habits listed below. `tools/check.sh` runs it.

## The rules

**Begin with the user’s task.** Explain when the reader needs this page before introducing
commands, record formats or internal names. For example: “Find the callers before changing
this function’s arguments.”

**Explain a term through an example.** Keep exact command and field names unchanged. Explain
what they let the user do. A “cutoff” means the tool could not finish answering the question;
an unsupported import is a concrete example. The [glossary](terminology.md) supplies detail,
but a user should not need to read it before understanding a guide.

**Make the example useful.** State the user’s problem, show the relevant command or code,
then explain the result and its limits. Label an illustration as an illustration. Preserve
actual observations in recorded experiment reports.

**Short sentences.** 25 words is the limit. Most sentences are shorter. Split a long
sentence into two.

**Active voice.** Write "the index holds the symbol", not "the symbol is held by the
index". Passive voice is allowed when the actor does not matter.

**Simple tenses.** Use present tense for current behavior. Use past tense for dated observations
and completed experiments. Make the distinction clear.

**Separate instructions from run history.** A current guide tells the reader what to do with the
installed tool. A dated experiment tells them what happened in that attempt. Preserve unsuccessful
attempts and their original output. Link the current instructions from historical pages when needed.

**Say what is true.** Do not define a thing by what it is not. "A refusal writes no
files" is better than "a refusal is not a plan".

**No em-dash.** Use a full stop, a comma or a colon. An em-dash usually joins two
sentences that read better apart.

**No filler.** Delete "simply", "merely", "exactly", "actually", "of course", "in fact".
They add no information.

**Do not point at your own text.** Write the fact. Do not write "that is why", "which is
how" or "this is what makes".

**One topic in a paragraph.** Six sentences is the limit.

**Use a list** for three or more items, for conditions, and for steps in order.

## Tutorials

A tutorial must supply a complete starting point: required tools, files, setup and expected output.
Say whether a snippet can run by itself, continues an earlier snippet or only illustrates a concept.
Name the file to create and the folder from which to run a command.
Define placeholders and explain where each returned ID comes from before asking the reader to reuse it.

Teach the concept before its notation. Introduce a parameter as a named function slot before explaining positional-only syntax.
Explain a JSON pointer with `/body/0`, the first item in `body`, before presenting a long structured edit.
A link to a glossary supplements a local explanation; it does not replace one.

After the example, explain what should change, what should remain unchanged and how the reader checks it.
Show a relevant failure and next action when it affects the user's decision.
For example, a failed test after application can leave source edited; tell the reader to inspect history before retrying.

A tutorial is a lesson, and the reader is the student. The reader acts; the types are
what the reader changes; the checker is the tool that enforces the result. Do not cast
the checker as the pupil or the hero.

**Write from the reader’s point of view.** Use a concrete task: “You need to fix checkout
without changing its API.” Let the reader see the decision, action and consequence. Avoid
inventing people or stories that add detail without explaining the task.

**Say what can no longer exist.** The payoff of a type is that the bad program stops being
a program. Write "that call can't exist any more", not "the checker rejects it".

**Keep the three parts apart.** The paragraph above a code cell states the problem and
stops there. The cell answers it. The paragraph below draws the conclusion and gives the
run-time cost. Do not braid the three together.

**Describe only what the cell contains.** Every claim about an example is checked against
the file. Do not invent a detail to fit the story.

**Names cost the reader.** A person's name or another language's name in the middle of a
lesson buys nothing. Credit belongs in the concepts table. Quote somebody's words and the
credit stays with the quote.

## Sentence rhythm

A device used once is a pleasure and used ten times is a tic. Count these before
committing prose:

- **Ending on a negation.** "and never complains", "and nothing notices". One per page.
- **The balanced pair.** Two mirrored clauses, one after the other. Break the second.
- **The three-part list inside a sentence.** Keep one per document.
- **The trailing sting.** A comma and a short verdict added after the sentence has ended.
- **The three-word beat.** "The program took it." A short sentence should carry a fact.

Read the sentence aloud. If it sounds like a line from a talk, write the plain version.

## Phrases a language model reaches for

These arrive ready-made and read as filler to anyone who has seen them before. The list
grows. Add to it whatever gets caught in review.

| Avoid | Write |
| --- | --- |
| earns its keep, earns its place | is worth the cost, say what it buys |
| load-bearing | say what depends on it |
| the price of admission | what you have to do |
| does the heavy lifting | name the part that does the work |
| a testament to, delve, tapestry, landscape | (delete, then say the fact) |
| not just X, but Y | the positive claim on its own |
| an alliterated summary, "pence pass for pounds" | the plain statement |

## What a comment is for

**Default to no comment.** Most code needs none. Write one only where a reader who
knows the language would otherwise change the code and break something.

**Never say what the code does.** The code says it. A comment carries a reason, a
constraint, a unit, or a rule the code cannot state.

**One line.** Two where the constraint needs two. A paragraph means the code is wrong;
fix the code.

**Write active voice and imperative mood.** Say who acts and what to do. "Read both
halves" and not "both halves are read". "Trim the trailing newline" and not "the
trailing newline is trimmed here". This holds for every comment, message, document and
tutorial in the repository.

**Write for a reader who arrives today.** Never the defect that led here, never what an
earlier version did, never what somebody once tried. `BUGS.md` holds defects and the git
log holds changes.

**Give no justification.** A comment states the constraint. It does not argue for the
code, weigh alternatives, or say what would happen otherwise.

| Delete | Keep |
| --- | --- |
| `// increment the counter` over `count += 1` | `// Bytes: tree-sitter reports byte offsets.` |
| `// The old behaviour added a dead import.` | `// Sort before comparing: the walker yields no order.` |
| `// Reading only the first turned X into Y, which is a wrong answer.` | `// Read both: X and Y mean opposite things.` |
| `// This used to refuse every Java call site.` | `// Java overloads; the name alone does not identify one.` |
| `// which is worse than declining the move` | `// Refuse: the move would change what the name means.` |

## Words to avoid, and what to write

| Avoid | Write |
| --- | --- |
| rather than, instead of | the fact on its own |
| utilise | use |
| in order to | to |
| terminate | stop, end |
| prior to | before |
| a number of | the number, or "some" |
| it should be noted that | (delete) |
| leverage (as a verb) | use |

## Jargon

Assume the reader knows how to program. Do not assume the reader knows this project,
tree-sitter, or the language being analysed.

Explain a term the first time a document uses it. Pair important technical details with a
user example. A glossary link supplies more detail; it cannot replace the explanation.

## Review before publishing

### Pull request descriptions

Explain the user's problem, the resulting behavior, one example, remaining limits and relevant checks.
Introduce the example's concepts before its code. Use plain words even when the implementation has precise internal names.
For example, describe a function importing another file before discussing dependency records.

Keep the description short enough to review without opening a report. Link to detailed test inventories and experiment history.
A small example's success cannot establish full library support or savings on complete agent tasks.
For older PRs, describe what that change established at the time. Later work can support more behavior.

### Guides and reports

Read the page without assuming knowledge of earlier PRs or reports.
Check every new concept at its first use, including tables, examples and tracking notes.
Keep exact command names, fields and stored outputs so readers can connect explanations to the tool.

For measurements, name the baseline and operation before giving a percentage.
Report both raw amounts, units, included setup and validation, repetitions, cache state and unknown costs.
Distinguish program execution overhead, tool runtime, text sent to an agent and complete-task cost.
The [measurement guide](benchmarks.md) shows the existing comparisons and their limits.

For tracking pages, state the user outcome, present status, evidence and next action.
Explain IDs through their user question and label historical observations with their date or run.
Update generated wording in its renderer so regeneration preserves these explanations.
Preserve original experiment outputs and failures; add context around them instead of rewriting their observations.

Ask whether the opening answers “When would I use this?” Read the example and check that
its commands exist and its interpretation matches the tool’s limits. Separate an unknown answer
from a negative answer, a preview from a tested repair, and a model proof from runtime behavior.

Keep one current report format. Do not imply that the SDK accepts older formats or fills in
missing metadata. Link readers to the current instructions and label historical observations.

Review the detailed sections too. A plain opening cannot explain a later paragraph that only lists
internal nouns. For each important field or restriction, explain its effect with a user example:

| Detail to explain | Example of a useful explanation |
| --- | --- |
| `complete: false` | “The import search stopped, so an empty trace cannot rule out the connection.” |
| A stale review | “Another edit changed checkout after your preview. Prepare and review a new diff.” |
| A passing command | “The build passed; run the fractional-price test to check the reported rounding error.” |
| An unsupported argument change | “Removing `record_purchase()` could remove a database write, so this route refuses.” |
| A recorded proof | “Lean accepted the access-rule model; its connection to the source still needs checking.” |

Keep one authoritative explanation of each behavior. When support changes, update that guide and
replace conflicting summaries with a link. Keep exact commands, field names and numerical limits intact.
