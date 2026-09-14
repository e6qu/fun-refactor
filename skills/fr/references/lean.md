# Check Lean evidence without overstating it

Ordinary use does not require Lean. `spec verify` requires Lean/Lake and an owning checked package.
Preview `fr --json spec init`, then apply with `--write` or save with `--save-plan`.

For supported pure functions, run `fr --json spec candidates src`, save
`fr --json spec plan TARGET --property KIND`, and apply it with
`fr --json spec scaffold --from PLAN --write`. Review each property. Use `spec goals specs`, reveal
one returned digest, then request its `spec proof-task`. The task contains empty templates; write the
tactics without `by` yourself. Run `spec proof-check` until Lean accepts them, then use `spec prove`
and `spec verify`. `docs/agent-formalization.md` defines the subset and evidence boundary.

Manual scaffolding remains available as `fr --json spec scaffold src/lib.rs::allowed`. It creates an
anchor, signature map and visible `sorry`; replace its model and add reviewed properties. On source
drift, rerun it and review the generated-region diff; the handwritten region is preserved. Keep debt
below `-- fr:debt NAME`, ratchet `spec check specs --strict --max-debt N`, generate CI with
`spec ci --max-debt N`, and report assumptions and obligations with `spec evidence specs`.

For an existing Rust declaration in `src/lib.rs`:

```rust
pub fn allowed(ok: bool) -> bool { ok }
```

An agent can author this small model in `specs/Model.lean` inside a configured Lake package:

```lean
-- fr:spec src/lib.rs::allowed @ 00000000
-- fr:signature ok: bool => ok: Bool; return: bool => return: Bool
def allowed (ok : Bool) : Bool := ok

theorem accepts_true : allowed true = true := by rfl
```

The zero hash starts stale. Inspect the source, model, types and property before accepting an identity.

```sh
fr --json spec check specs --strict
fr --json spec sync specs
fr --json spec sync specs --write
fr --json spec verify specs
```

`sync` only renews reviewed hashes; it does not repair models, signatures or proofs. Explicit maps
must match both declarations and currently support Rust sources. Verify checks them before each
`lake build --wfail`. Ensure the selected model belongs to a target; zero `sorry` does not imply
property completeness.

Keep four claims separate in the final evidence:

- A fresh anchor matches recorded source bytes.
- A fresh signature map matches the declared source and model surfaces.
- A Lean theorem proves its proposition under the model's definitions and assumptions.
- Shared execution cases test correspondence on those cases only.

None alone proves the full Rust implementation correct. State the covered domain, assumptions, axioms, trusted tools and remaining correspondence obligations.
Translation into Lean also does not supply an implementation-equivalence proof.
