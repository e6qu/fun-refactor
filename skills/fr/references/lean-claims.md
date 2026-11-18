# Create a model and claim

A model describes source behavior in Lean. For supported pure source declarations,
use `spec property-task TARGET`, `spec plan TARGET --property-from FILE` and
`spec scaffold --from PLAN --package specs --write`. Review the model and exact
requested claim, then follow the loop above. `spec verify specs` builds its package.
A model proof leaves source/model equivalence open, including overflow and widths.

For admitted Rust scalar functions, `spec plan TARGET --property rust-scalar-model`
adds an arithmetic connection to prove. It compares checked evaluation with the model
for all bounded inputs. Parsing, extraction and the operational rules remain trusted.
Read `spec evidence` for the exact connection and its remaining obligations.

Return to [Proofs](lean.md) after reviewing the scaffold. A fingerprint identifies bytes; renewing it cannot repair a wrong model or proof.
