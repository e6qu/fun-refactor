# First real-agent acceptance evaluation

Four fresh agents completed two tasks on strsim 0.11.1, with one fr trial and one ordinary-file trial per task.
Both fr trials completed discovery, editing, declared checks, patch export, undo/redo and application in a clean receiver.
Independent behavioral oracles passed on every final project and receiver.
The fr trials consumed more retrieved context than the ordinary-file trials on this small project.

## Results

| Task | Tool surface | Passed | Retrieved context tokens | Tool calls | Trial seconds |
|---|---|---|---:|---:|---:|
| Unicode Sørensen–Dice fix | fr | Yes | 18,628 | 31 | 166.9 |
| Unicode Sørensen–Dice fix | Ordinary files | Yes | 9,469 | 22 | 119.7 |
| Normalized OSA API | fr | Yes | 17,370 | 32 | 156.5 |
| Normalized OSA API | Ordinary files | Yes | 10,393 | 22 | 107.4 |

All four trials had zero recorded tool refusals or failures and zero human task corrections.
Each left only `src/lib.rs` changed among tracked files. Both indexes retained their exact bytes.
Undo restored original tracked bytes and modes; redo restored the final result. An unrelated later edit survived both operations.

The Unicode oracle checks 67,081 input pairs against independently sorted scalar bigram multisets.
The OSA oracle checks 7,569 input pairs against a separate dynamic-programming implementation.
Both include Unicode and empty inputs. OSA cases distinguish restricted transpositions from unrestricted Damerau–Levenshtein distance.
The original release fails the Unicode oracle and lacks the requested OSA API; passing upstream tests alone cannot satisfy either task.

The upstream code contains 41,054 Rust source bytes across its released library, integration tests and benchmark.
No artificial background source increases that denominator.
See [source provenance](../tests/agent-eval/PROVENANCE.md) and the [evidence manifest](../tests/agent-eval/results/2026-09-07/manifest.json).

## What the token count means

The primary metric adds the task prompt and every visible instrumented tool payload once.
Skill reads count when they occur. Generated tool-request tokens have a separate field in each result.
The metric excludes system instructions, assistant commentary, hidden reasoning, tool transport framing and repeated conversation-prefix processing.
It measures retrieved task context under a reference encoding, without claiming serving-model billing or peak context usage.
The facade reserializes JSON and adds status fields; direct CLI transport can have different overhead.
Paths, timings and revisions contribute tokens, so a new trial can yield different counts.

The tokenizer is tiktoken 0.12.0 with o200k_base.
The vocabulary SHA-256 is `446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d`.
Scoring refuses a different package version or vocabulary and needs no network once the vocabulary exists locally.

| Visible output category | Dice fr | Dice files | OSA fr | OSA files |
|---|---:|---:|---:|---:|
| Skill reads | 3,848 | 0 | 3,848 | 0 |
| Project/source inspection | 6,506 | 3,294 | 5,935 | 4,288 |
| Declared checks | 3,977 | 4,785 | 3,679 | 4,528 |
| Changes and delivery | 3,464 | 565 | 3,064 | 741 |

The fr trials used 96.7% and 67.1% more retrieved context, respectively.
Skill overhead does not explain the whole difference: project inspection and transaction reporting also cost more here.
The traces include broad file maps and repeated maps after writes.
The next optimization should target bounded declaration lookup, avoid unnecessary handle refreshes and reduce successful validation/reporting output.
Another paired run must measure any resulting improvement before the docs claim savings.

## Trial protocol and limits

Each trial starts a fresh collaboration agent without conversation history or project-specific hints beyond the task prompt.
Agents inherit the same parent model and effort, with no overrides. The platform does not expose a pinned model-build identifier in this evidence.
Fresh context prevents contamination from sibling trials; it cannot establish absence of training-time knowledge of the public project.

The fr arm reads source through project queries and edits through saved authoring transactions.
The ordinary-file arm uses bounded file listings, line reads, literal searches and text replacement or append operations.
Both arms use the same declared-check command. The ordinary-file arm exports and reverses patches with Git.
The fr arm exports through source history and uses its undo/redo operations.
Both must check original, changed, undone and redone states in order before verifying the receiver.

Agents use a cooperative instrumented facade. It records requests, visible results, timings and tracked source snapshots at each call.
This is a constrained comparison with ordinary file tools, not unrestricted shell access or an enforced security sandbox.
The facade can perform reads and artifact writes; high-level fr operations still run the real CLI binary.
Agents choose their own inspections, code and command sequence. They cannot inspect evaluator oracles or sibling sessions.

Three initial pilot agents hit Cargo workspace inheritance because their projects lived beneath the fr checkout.
Those agents stopped; the valid trials used fresh agents in independent temporary directories.
The harness now rejects Cargo ancestors and preflights upstream tests before handing over a project.
The evidence includes the pilot transcripts; their infrastructure failures do not enter the four scored results.

There is one valid trial per arm and task, on one small Rust project and one host.
Trials overlapped each other and repository validation, so timing includes shared-machine contention.
The skill entry and optional references form additional context in the fr arm, as they do in a first handoff.
Results establish these concrete workflows. They do not establish a general success rate, context reduction, large-project scaling or framework-migration readiness.
The Rust compiler and tests supply behavioral evidence; no new Lean proof covers these project tasks or the evaluation harness.

## Reproduction

Replay the recorded patches, upstream tests, independent oracles and source-transition evidence:

```sh
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-07
python3 tools/agent_eval/test_harness.py
```

The native test gate runs both through `tests/agent_acceptance.rs`.
Replay verifies the evidence checksums and requires the exported patches to reproduce each recorded final source hash.
It checks reverse/reapplication, unrelated-edit preservation and index identity in new temporary repositories.
Replay executes recorded changes. A fresh autonomous trial requires an agent runtime.

Prepare another set outside any Cargo project:

```sh
cargo build --bin fr
python3 tools/agent-eval.py prepare --out /tmp/fr-acceptance-new --fr target/debug/fr
```

Give each generated `prompt.txt` to a separate fresh agent and require its instrumented tool protocol.
Keep the binary unchanged until every trial finishes; each invocation checks its digest.
Do not share solutions or source context across arms. Record any intervention before interpreting results.
The `manual_corrections` field in session.json records task corrections when a future run needs them.

Install and populate the optional tokenizer environment once:

```sh
python3 -m venv target/agent-eval-venv
target/agent-eval-venv/bin/python -m pip install -r tools/agent_eval/requirements.txt
TIKTOKEN_CACHE_DIR=target/agent-eval-tokenizer target/agent-eval-venv/bin/python -c 'import tiktoken; tiktoken.get_encoding("o200k_base")'
```

Score each completed session with that environment, then retain the four-trial bundle:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py score /tmp/fr-acceptance-new/unicode-dice-fr
python3 tools/agent-eval.py record /tmp/fr-acceptance-new /tmp/fr-acceptance-evidence
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-07
```

Repeat scoring for the other three session directories before recording a complete evidence bundle.
Recording preserves scored failures too; replay still fails for unsuccessful trials. Preserve all results when reporting an experiment.
The optional `record --pilots DIRECTORY` includes interrupted infrastructure runs in the archive.
The evidence manifest records tool versions, implementation commit, binary and archive digests, and evaluator file hashes.
