# Correct a failed edit and deliver the result

A failed applied check can mean that the replacement body is wrong. Fixing a check prerequisite
cannot repair that source. Keep the failed report, inspect the current transaction, and review
a corrected body against the source that is actually present.

For example, an edit meant to uppercase text might accidentally return an empty string.
The failed transaction records the change from `value.strip()` to `''`. A second transaction
can replace that incorrect body with `value.strip().upper()`.

There are two existing routes:

| Route | Source used for the corrected edit | Patch delivery |
| --- | --- | --- |
| Undo and correct | Original source, after a reviewed undo | Export the corrected transaction |
| Repair the applied source | Incorrect source retained by the failed transaction | Export both transactions in their original order |

Neither route resumes the old transaction. [Applied-state resume](workflow-recovery.md) requires
unchanged recorded source. A corrected body needs fresh handles, a fresh review, and a new transaction.

Prefer undo-and-correct when the original source can be restored safely. In this comparison,
it used fewer calls and fewer serialized protocol bytes. Existing exports also supported the
ordered-patch route; these results do not justify another public operation.

## Keep the verification policy explicit

The incorrect intermediate source should fail its behavior check. A repair workflow therefore
cannot require that starting state to pass. The comparison uses this delivery policy for both routes:

```python
TaskDelivery(
    check_original=False,
    exercise_reversal=False,
    check_output_bytes=512,
)
```

This policy still checks the applied correction. It requests no patch. The caller separately
verifies the complete original-to-final lifecycle before exporting:

1. Preserve the failed transaction and its required check configuration.
2. Review and apply the corrected body. Stop if its applied check fails.
3. Preview and undo the delivery transactions in reverse order.
4. Check the restored original source and its revision.
5. Preview and redo those transactions in forward order.
6. Recheck the final source and record the receipt on the corrected transaction.
7. Export the transactions in forward order and concatenate their patch text into a new file.
8. Apply that file to an independent original checkout and check its behavior.

For repair-on-applied-source, the delivery chain contains the failed and corrected transactions.
For undo-and-correct, it contains only the corrected transaction. The caller retains every failed
attempt and every review, transition, check, export and submission in its accounting.

Preserve any required acceptance checks as well. The comparison has one behavior check;
it does not authorize dropping another caller's compiler, test, proof or delivery requirements.

## Patch and interruption limits

Concatenation creates one patch file, but preserves both edits. It is not a minimal diff and
contains the incorrect intermediate body. Receivers must start from the original source and apply
the complete ordered patch. A repair-only patch expects the incorrect intermediate source.

Patch export describes recorded snapshots. It does not certify that current source passes checks.
The complete caller must enforce the verification sequence above. Inspect the exported transaction
IDs and order; do not concatenate unrelated transactions or infer completeness from adjacent IDs.

The sequence is not one atomic operation. A conflict or interruption can leave a transaction
undone or applied without delivery. Inspect retained history before continuing. Preserve later
edits when undo refuses. Pending filesystem writes still use the separate history recovery commands.

The patch destination is create-only. Export into a disposable staging location and publish only
after receiver verification. The comparison's receiver has no model-supplied shell commands.
Its Python or compiled Rust oracle checks the final behavior independently of the edit planner.

## Body-helper adoption review

`guide.source_body_action(...)` prepares caller-authored bodies for every exact guided handle.
It preserves the goal's base checks, delivery and proof setting. It adds exact file, edit,
changed-operation and path counts. Its diff budget comes from the goal's token-limit field,
whose maximum is 4,096 bytes; the field name does not make this a token measurement.

| Current caller | Policy that must survive | Decision |
| --- | --- | --- |
| [Guided body comparison](../tools/guide-body-context.py) and this recovery comparison | Exact targets, all changes required, bounded complete review, declared checks | Use the helper; retain full caller accounting |
| [Multiple-body runner](../tools/upstream-multibody-agent.py) | 65,536-byte diff, typecheck, exact two files/edits; no changed-operation count | Keep its explicit action |
| [Single TSX-body runner](../tools/upstream-tsx-body-agent.py) | 65,536-byte diff, typecheck, exact file/edit; no changed-operation count | Keep its explicit action |
| [Python repository tasks](../tools/python-repository-acceptance.py) | Syntax through reversal; separate behavior/upstream acceptance checks; 8,192-byte check output | Keep the typed task and separate acceptance policy |

Changing review budgets or adding a changed-operation requirement changes admission, even if a
current fixture still passes. Replacing these callers would need a measured policy-preserving
comparison. Frozen historical runner snapshots remain unchanged.

## Retained results

[Run 38055066754](https://github.com/e6qu/fun-refactor/actions/runs/38055066754) built the CLI and
passed all 40 comparison cells and 16 corruption tests. Execution took 2 minutes 25 seconds;
runner queue time was separate. The [retained report](../tests/agent-eval/results/2026-10-10-source-repair/result.json)
contains both complete programs, every recorded fr request and response, and the receiver checks.
Its adjacent manifest verifies the archive, executable, source commit and report digest.

Eight successful deliveries passed original and final checks, complete reversal, receiver replay,
independent behavior and actual conflicting-apply controls. The other 32 cells retained wrong
repairs, stale source or check declarations, and later conflicting edits without delivering a patch.
The ordered-patch route also rejected repair-only and reversed-order patches on original receivers.

| Edit shape | Repair total caller bytes | Undo/correct total caller bytes | Repair total protocol bytes | Undo/correct total protocol bytes |
| --- | ---: | ---: | ---: | ---: |
| One Python body | 4,514 | 4,645 | 43,677 | 42,356 |
| One Rust body | 4,523 | 4,654 | 43,543 | 42,131 |
| Two Rust bodies | 4,695 | 4,826 | 51,262 | 49,615 |
| Two Python files, same function name | 4,705 | 4,836 | 53,607 | 51,679 |

Repair used 20 total fr calls; undo-and-correct used 17. Both totals include three calls from
the initial failed edit. Caller bytes include both programs, transaction input and canonical
submission. Protocol bytes count serialized recorded requests and responses across both attempts.
Common fixture setup, recording machinery and receiver oracles are outside these byte counts.

The repair caller is 131 bytes shorter, but uses more calls and protocol bytes in every fixture.
That is a tradeoff, not an established agent-efficiency improvement. Bodies are prescribed;
no model, token, elapsed-task-time or provider-cost comparison ran. These results do not establish
arbitrary transaction composition, atomic multi-transaction recovery or workstation client admission.
