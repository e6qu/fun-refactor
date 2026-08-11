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

## What a comment is for

A comment says what the reader cannot see in the code.

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
