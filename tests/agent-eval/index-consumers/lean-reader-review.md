# Why the repository benchmark answers changed

The benchmarks read two fixed copies of this project. A reader improvement can find code
that the earlier reader missed, even though those source files have not changed.
We preserve the original timing baselines and record the changed answers separately.

For example, in `simp only [lemma] at accepted ⊢`, `only` and `at` instruct the Lean
simplifier; they are not variables used by the program. Earlier parsing sometimes treated
them as expressions and continued into the next declaration. The repaired grammar keeps
these proof instructions together and recovers the later declarations.

Two GitHub runs built both readers independently and read the same source bytes:
[consumer snapshot review](https://github.com/e6qu/fun-refactor/actions/runs/40477575144) and
[indexing snapshot review](https://github.com/e6qu/fun-refactor/actions/runs/40478787425).
The exact reader commits, source hashes and every changed location are in
[the retained inventory](lean-reader-review.json).

Inline `show … by …` proof terms also retain the previous reviewed facts and syntax results in both fixed copies.
The consumer benchmark retains its 512 recorded questions.
The original timing baselines stay unchanged; current timings need the rebuilt executable.

The subscript-name and explicit-conversion repairs passed two further comparisons:
[consumer source copy](https://github.com/e6qu/fun-refactor/actions/runs/40713786808) and
[indexing source copy](https://github.com/e6qu/fun-refactor/actions/runs/40716624923).
Complete source bytes, code facts, syntax results and the comparison driver's question selections remain identical before and after.
The [follow-up record](reader-recheck-2026-10-28.json) retains both reader revisions, the exact counts and artifact hashes.
Its driver sampled 512 questions in the first copy and 511 in the second.
These samples are separate from the unchanged 512 recorded consumer questions linked below.
The original correction inventory and timing baselines remain unchanged.

The repairs to reading division facts and building reports also preserve both fixed source copies.
The [2026-10-29 review](reader-recheck-2026-10-29.json) retains the two successful checks and exact file hashes.
All source bytes, code facts, syntax results and sampled questions remain identical before and after.
The original correction inventory, recorded consumer questions and timing baselines stay unchanged.

The [introduction-pattern review](lean-reader-introductions.json) records one further correction on both fixed copies.
An assumption is a fact available inside a proof. In `rintro ⟨ha, -, -, -⟩`, Lean keeps the first fact as `ha` and discards the others.
The earlier reader counted the word `rintro` as an ordinary identifier at two locations in `ProjectIdentity.lean`.
The corrected reader recognizes the proof instruction and removes those two false references.
All declarations, resolved-reference counts, source bytes, syntax diagnostics and sampled question selections stay unchanged.
The current inventory retains every earlier correction and the first inventory's exact hash; the first inventory itself is unchanged.

| Fixed source copy | References before this correction | References after this correction |
| --- | ---: | ---: |
| Consumer benchmark | 592,648 | 592,646 |
| Indexing benchmark | 593,974 | 593,972 |

The original timing baselines remain unchanged. Current timings must use newly collected results with the corrected reader.
The driver still samples 512 questions in the consumer copy and 511 in the indexing copy.
The separate saved set of 512 consumer questions also stays unchanged.

| Fixed source copy | Before the original reader repair | After the original reader repair |
| --- | ---: | ---: |
| Consumer benchmark declarations and parameters | 98,147 | 98,231 |
| Consumer benchmark reference occurrences | 592,956 | 592,648 |
| Indexing benchmark declarations and parameters | 98,355 | 98,439 |
| Indexing benchmark reference occurrences | 594,282 | 593,974 |

The original baselines each contain six more references than the “before” column.
Those six were false variable reads of `_` in patterns such as `rcases value with _ | other`.
Their earlier correction and exact source locations remain checked separately.

Both copies have the same Lean files. The repair adds 84 declarations and parameters and
removes none: 18 functions and 62 parameters in `Workspace.lean`, plus one function and its
parameter in each of `Source.lean` and `Project.lean`. Thirteen existing declaration records
change their extent or enclosing declaration. Reference changes remove 484 records, add
176 records and change the target or classification details of 311 otherwise matched records.
A removed record paired with an added record at the same source location can be a change
from a plain name to a function call, rather than a lost occurrence.

For example, `change n ≤ limit` asks Lean to restate the current goal.
The reader previously ended the proof after `change n` and misread the remaining comparison.
It now keeps the comparison inside that proof step. This removes the parser errors in
`Project.lean` and `Patch.lean` and two false variable reads of the tactic names `rcases` and `rw`.
No declaration disappears, and no previously clean file gains a parser error.

The checks hash all current Lean declarations, parameters and references using source
locations instead of temporary numeric IDs. They require the reviewed digest, all 59 Lean
file hashes and the exact counts. The unchanged smaller workloads still require their
original complete answers. Each indexing repetition must produce the same full answer
digest. Each consumer answer must agree with a separate linear search of the current index.

The consumer test previously selected every nth declaration. Recovering missing declarations
silently changed 445 of its 512 questions. It now selects the
[same 512 recorded source definitions](repository-queries.json), failing if any is missing or
ambiguous. The old timing baseline did not retain its chosen locations, so we do not claim
to reconstruct that historical selection or an identical-question speed improvement.

This is a check of the recorded syntax facts, not proof that the reader understands every
Lean construct. Generic proof instructions can still appear as identifier references, and
some Lean source forms still contain parser errors. These measurements include indexing,
answer construction and language-support changes. They do not establish a runtime speedup
for user programs, agent savings or equivalence between a source program and its Lean model.
