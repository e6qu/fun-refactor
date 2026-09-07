# Agent context reduction follow-up

Four fresh agents repeated the two strsim tasks after the M4i lookup, check-output and skill changes.
All four passed. The fr trials retrieved 34.0% and 24.5% fewer tokens than the [first fr trials](agent-acceptance.md).
The new ordinary-file trials also used less context, and fr still consumed more than those baselines.
These results close this measured optimization milestone; broader context efficiency remains open.

## Changes evaluated

Implementation commit `0498c6b` adds three related improvements:

- `fr project find NAME` returns paged declaration handles, parents and optional signatures within a selected subtree.
- `fr checks --run NAME --basis DIGEST --quiet-success` omits successful stream text while retaining outcomes, omission counts and failed-command diagnostics.
- The portable skill loads authoring separately from recipes and patch delivery separately from Git administration.

Lookup defaults to exact, case-sensitive names. `--contains` selects literal substring matching.
Matching uses full names before display clipping. Coverage and hidden-local counts distinguish omitted facts from absent matches.
Cursors bind to the revision, scope and query options. Source edits still invalidate project handles.
History and check commands need no replacement handles when they do not query source.
Lookup bounds returned context; it still builds the project index and does not establish lower indexing cost.

The trial prompt now says to keep handles revision-bound when using them, replacing the unconditional refresh instruction.
Both arms received that clarification. Task requirements, independent oracles and mandatory validation stages stayed the same.
The trials evaluate these changes together and cannot assign an exact causal reduction to each feature.

## Paired results

| Task | Tool surface | Passed | Retrieved context tokens | Tool calls | Trial seconds |
|---|---|---|---:|---:|---:|
| Unicode Sørensen–Dice fix | fr | Yes | 12,287 | 29 | 190.7 |
| Unicode Sørensen–Dice fix | Ordinary files | Yes | 6,087 | 18 | 115.7 |
| Normalized OSA API | fr | Yes | 13,109 | 31 | 166.8 |
| Normalized OSA API | Ordinary files | Yes | 8,397 | 17 | 86.9 |

The fr counts fell from 18,628 to 12,287 for Dice and from 17,370 to 13,109 for OSA.
Against the new file trials, fr used 101.9% and 56.1% more retrieved context, respectively.
The first file trials used 9,469 and 10,393 tokens. Their improvement matters when interpreting the comparison.
The fr trial times increased despite fewer calls; no speedup follows from this run.

All four trials had zero recorded tool failures or refusals and zero human task corrections.
There were no interrupted pilots or infrastructure restarts in this follow-up.
Each trial passed upstream tests in original, changed, undone and redone states, in that order.
Undo and redo restored exact tracked bytes and modes while preserving an unrelated later edit.
Only `src/lib.rs` changed among tracked files; project and receiver indexes stayed identical to their initial bytes.
Exported patches reproduced the final source in clean receivers.

Independent oracles passed 67,081 Unicode Dice pairs and 7,569 OSA pairs in each corresponding project and receiver.
The original release still fails the Dice oracle and lacks the OSA API, so upstream tests alone cannot satisfy the tasks.
The project remains the complete published strsim 0.11.1 archive, with 41,054 Rust source bytes and no artificial background source.
The [provenance](../tests/agent-eval/PROVENANCE.md) and [follow-up manifest](../tests/agent-eval/results/2026-09-07-context/manifest.json) identify the source and recorded evidence.

## Where context went

| Visible output category | Dice fr | Dice files | OSA fr | OSA files |
|---|---:|---:|---:|---:|
| Skill reads | 2,735 | 0 | 2,735 | 0 |
| Project/source inspection | 3,741 | 2,639 | 4,670 | 4,794 |
| Declared checks | 1,796 | 2,070 | 1,796 | 2,070 |
| Changes and delivery | 3,181 | 552 | 3,063 | 696 |

The fr skill reads fell from 3,848 to 2,735 tokens per task.
Both agents used targeted lookup and quiet-success checks, and neither refreshed project handles after its final source write.
Both first searched a partial name in exact mode, then retried with `--contains` when no declaration matched.
This points to a remaining opportunity in query selection guidance.
Transaction previews, history operations and skill reads still add substantial context on these small tasks.
Further work should use these traces to reduce repeated information without removing outcomes or reviewable edit details.

## Measurement limits and validation

The reference count adds the prompt and each visible instrumented tool payload once, including skill reads.
Separate fields record generated request tokens. The count excludes system context, hidden reasoning, commentary and transport framing.
The measurement omits repeated prefix processing, caching and billed usage.
The tokenizer remains tiktoken 0.12.0 with the pinned o200k_base vocabulary described in the first report.
An exact token audit reproduces all four scores from the retained transcripts.

Each arm and task had one fresh agent, with inherited model and effort and no overrides.
The evidence does not identify a pinned serving-model build. Agents used the same cooperative tool boundary as the initial evaluation.
Trials overlapped each other and repository validation on one host, so timing includes shared-machine contention.
Task choices, paths and output timings vary between runs. One small project cannot establish general savings or a success rate.

The full native/WASM gate passes, including 127 project CLI scenarios, ten check scenarios and all 33 executable skill examples.
Capability coverage remains 311/311. Strict verification reports twenty-two fresh anchors and signature maps, zero obligations and 29 Lean build jobs.
Lookup reuses the existing modeled page-length kernel. These results add no proof of parser correctness, process execution or agent-authored task behavior.

## Reproduction

The standard acceptance regression replays both evidence bundles, covering eight recorded patches and ten harness regressions.
Replay checks evidence hashes, upstream tests, independent oracles, reverse/reapplication, exact source identity and index preservation.
It reproduces recorded behavior; a new autonomous trial requires fresh agents.

```sh
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-07-context
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-07-context
```

See the [initial reproduction instructions](agent-acceptance.md#reproduction) for preparing agents and the optional tokenizer environment.
Both bundles retain their original prompts, transcripts, scores, patches and skill snapshots.
The follow-up manifest records implementation commit `0498c6b`, the frozen binary digest and evaluator hashes.
Future experiments should include larger projects and repeated trials before extending claims beyond these two tasks.

## Subsequent history completion reports

M4j adds opt-in `--no-diff` for history writes after the agent reviews a plan or transition preview.
It retains transaction identity, action, applied status, paths, existence and modes, with `diffs_omitted: true`.
The option requires `--write`. Previews and patch export keep their diffs; source snapshots and transition checks remain intact.

A controlled replay uses both retained fr edits, applying each with default reports and with `--no-diff` in fresh repositories.
It verifies identical report metadata, exact source bytes and modes, unrelated-edit preservation, unchanged indexes and identical exported patches.
The script measures three completion reports and the undo/redo previews, keeping both previews in both modes.
It uses direct CLI stdout, including its formatting, with the same pinned reference tokenizer as the earlier experiment.

| Task | Default completion tokens | With `--no-diff` | Default including previews | With `--no-diff`, including previews |
|---|---:|---:|---:|---:|
| Unicode Dice | 825 | 249 | 1,375 | 799 |
| Normalized OSA | 609 | 249 | 1,015 | 655 |

Completion output falls by 69.8% and 59.1% in these two fixed edits.
Including both previews, history transition output falls by 41.9% and 35.5%.
These counts exclude task prompts, skills, inspection, authoring plans, checks, patch export and generated requests.
This controlled measurement isolates output selection; it does not run fresh agents or measure total task context.
The earlier paired-agent scores remain unchanged. A future paired evaluation must test whether agents choose this option effectively.

The [retained report](../tests/agent-eval/history-context.json) includes the actual stdout strings, measurements and binary and input-manifest digests.
Run the comparison without the optional tokenizer, or include reference tokens:

```sh
python3 tools/history-context.py --fr target/debug/fr
target/agent-eval-venv/bin/python tools/history-context.py --fr target/debug/fr --tokens
```

The native acceptance regression runs the byte comparison without requiring tiktoken.
Dedicated history CLI tests cover omitted diffs, metadata, exact undo/redo, deletion/restoration, modes and unchanged conflict refusals.
Interrupted-write tests exercise smaller recovery reports alongside exact snapshot restoration.

The M4j full native/WASM gate passes, with 311/311 capability coverage and all 33 executable skill examples.
Strict verification retains twenty-two fresh anchors and signature maps, zero obligations and 29 Lean build jobs.
