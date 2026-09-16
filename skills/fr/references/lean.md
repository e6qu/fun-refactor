# Check Lean evidence without overstating it

Ordinary use does not require Lean. `spec verify` requires Lean/Lake and an owning checked package.
Preview `fr --json spec init`, then apply with `--write` or save with `--save-plan`.

For typed pure declarations across the shared readers, run `fr --json spec candidates src`, save
`fr --json spec plan TARGET --property KIND`, and apply it with
`fr --json spec scaffold --from PLAN --write`. Review each property. Use `spec goals specs`, reveal
one returned digest, then request its `spec proof-task`. The task contains empty templates; write the
tactics without `by` yourself. Run `spec proof-check` until Lean accepts them, then use `spec prove`
and `spec verify`. `docs/agent-formalization.md` defines the subset and evidence boundary.

For custom properties, request `spec property-task TARGET`. Author `fr-formal-property-1` with its
model signature, grammar and digest; the Python property classes mirror that tree.
Pass it to `spec plan TARGET --property-from FILE`. Keep Lean text and tactics outside property IR;
`fr` validates and elaborates, then the agent writes tactics through the proof-task loop.

Declarative file targets use `PATH::__fr_structure__` for retained spans, name containment and
ordered parent provenance. Inspect omissions; rendering and embedded execution remain unproved.
Choose `retained-facts-wellformed` for validity or `ir-model` for Boolean kernel/model correspondence.
The latter accepts at most eight Boolean inputs and a Boolean output. Write its tactics yourself;
strict checks pin the semantic library. `evidence.kernel_correspondence` identifies checked model
relations; `source_implementation_proved` stays false.

For direct bounded IR execution, use `spec kernel --from FILE` or `FrClient.kernel(KernelRequest(...))`.
Import `KernelTerm` and `KernelValue` from `fr_ir.formal_kernel`; they mirror the tagged wire tree.
Evaluation uses checked signed-64 arithmetic and explicit partial failures. Generated `Int`/`Nat`
source models use Lean arithmetic, so numeric correspondence requires its own evidence.

Manual scaffolding remains available as `fr --json spec scaffold src/lib.rs::allowed`. It creates an
anchor, signature map and visible `sorry`; replace its model and add reviewed properties. On source
drift, rerun it and review the generated-region diff; the handwritten region is preserved. Keep debt
below `-- fr:debt NAME`, ratchet `spec check specs --strict --max-debt N`, generate CI with
`spec ci --max-debt N`, and report assumptions and obligations with `spec evidence specs`.

A manual source/model pair:

```rust
pub fn allowed(ok: bool) -> bool { ok }
```

```lean
-- fr:spec src/lib.rs::allowed @ 00000000
-- fr:signature ok: bool => ok: Bool; return: bool => return: Bool
def allowed (ok : Bool) : Bool := ok

theorem accepts_true : allowed true = true := by rfl
```

Review source, types, model and property before renewing its stale anchor:

```sh
fr --json spec check specs --strict
fr --json spec sync specs
fr --json spec sync specs --write
fr --json spec verify specs
```

Hash renewal repairs no model, signature or proof. Verify the owning checked target.

Keep four claims separate in the final evidence:

- A fresh anchor matches recorded source bytes.
- A fresh signature map matches the declared source and model surfaces.
- A Lean theorem proves its proposition under the model's definitions and assumptions.
- Shared execution cases test correspondence on those cases only.

None alone proves the full source implementation correct. State the covered domain, assumptions, axioms, trusted tools and remaining correspondence obligations.
Translation into Lean also does not supply an implementation-equivalence proof.
