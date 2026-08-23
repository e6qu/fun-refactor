# How to write here

This project writes comments, messages and documents in a controlled style. The style
comes from ASD-STE100, Simplified Technical English. Aerospace maintenance manuals use
that standard so that a reader with limited English can follow a procedure without a
mistake. The same rules make a codebase readable to someone who arrives at it today.

`tools/check-prose.py` counts the habits listed below. `tools/check.sh` runs it.

## The rules

**One word for one thing.** `docs/terminology.md` lists the terms. If a term is there,
use it. Do not use a second word for the same idea. If you need a new term, add it to
that file first.

**Short sentences.** 25 words is the limit. Most sentences are shorter. Split a long
sentence into two.

**Active voice.** Write "the index holds the symbol", not "the symbol is held by the
index". Passive voice is allowed when the actor does not matter.

**Simple tenses.** Use the present tense for what the code does. Use the past tense only
for events, and only in `BUGS.md` and commit messages.

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

A tutorial is a lesson, and the reader is the student. The reader acts; the types are
what the reader changes; the checker is the tool that enforces the result. Do not cast
the checker as the pupil or the hero.

**People make the mistakes.** Give the story a person with a name and let the mistakes be
theirs. A named clerk who typed `darft` gives every later sentence a subject. "A status is
misspelled" has no one in it, and prose without actors slides back into passive voice.

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

A comment says what the reader cannot see in the code.

**Say it in the fewest words that still sound human.** Full sentences where a sentence
is needed. No throat-clearing, no editorialising, no calling a thing clever or a shame.

**Do not say what the code does.** The code says that. A comment gives the reason, the
constraint, the unit, or the rule that the code alone cannot state.

**Write for a reader who arrives today.** A comment is timeless. It describes the code
as it stands, never the defect that led to it, never what an earlier version did, never
what somebody once tried. `BUGS.md` holds defects and the git log holds changes.

| Avoid | Write |
| --- | --- |
| The old behaviour added a dead import. | (delete: the rule above it already says what to do) |
| Reading only the first turned X into Y, which is a wrong answer. | Read both, because X and Y mean opposite things. |
| which is worse than declining the move | (delete, or state the consequence plainly) |
| This used to refuse every Java call site. | (delete) |

Delete a comment that repeats the code. `// increment the counter` above `count += 1`
tells the reader nothing.

Keep a comment that gives a reason, a constraint or a unit. "Bytes, because tree-sitter
reports byte offsets" is worth a line.

Do not write the history of a defect in a comment. `BUGS.md` holds defects and the git
log holds changes. A comment describes the code as it is today.

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

Explain a term the first time a document uses it, or link to `docs/terminology.md`.
