# Check Lean evidence without overstating it

Ordinary `fr` use does not require Lean. `spec verify` requires Lean/Lake and an owning package whose checked targets include the selected model.
Start an unconfigured project with `fr --json spec init`, review the three-file package diff, then use `fr --json spec init --write` or `--save-plan`.
Initialization pins the supported toolchain and records the checked package through source history. Broader source-language scaffolding remains roadmap work.

For a Rust function with supported primitive, reference, tuple, `Option`, `Result`, `Vec` or `Box` types, preview `fr --json spec scaffold src/lib.rs::allowed`.
The write creates a full anchor, strict signature map and checked module with one visible handwritten `sorry`. Replace it with reviewed model semantics and add the property before expecting `spec verify` to pass.
After source drift, rerun the same scaffold command. It previews changes to the marked generated region and preserves the marked handwritten region byte for byte. Review any signature change and repair the preserved model or proofs before writing.

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

The zero hash deliberately starts stale. Inspect `actual`, the source declaration, model, types and claimed property before accepting a new identity.

```sh
fr --json spec check specs --strict
fr --json spec sync specs
fr --json spec sync specs --write
fr --json spec verify specs
```

The first command refuses this initial stale anchor. That is useful drift evidence, not a reason to suppress checks.
`sync` previews hash changes; `--write` records them through source history. It does not repair model definitions, signatures or proofs.
After a source change, first decide whether the claim or model needs updating. Renew hashes only after that review.
An explicit signature map must agree with both declarations; strict signature checking currently supports Rust source only.
Verify requires those checks to pass before running `lake build --wfail` for each selected package.
Lake may write build artifacts; a successful run must actually include the selected model in its build targets.
A zero `sorry` count alone is not evidence that every relevant property has a proof.

Keep four claims separate in the final evidence:

- A fresh anchor matches recorded source bytes.
- A fresh signature map matches the declared source and model surfaces.
- A Lean theorem proves its proposition under the model's definitions and assumptions.
- Shared execution cases test correspondence on those cases only.

None alone proves the full Rust implementation correct. State the covered domain, assumptions, axioms, trusted tools and remaining correspondence obligations.
Translation into Lean also does not supply an implementation-equivalence proof.
