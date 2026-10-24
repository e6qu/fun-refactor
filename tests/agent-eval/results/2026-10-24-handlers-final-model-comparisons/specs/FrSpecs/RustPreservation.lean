-- fr:comparison 25df368792fa203847aac380451108917e2897532445ac1d82a6caf150c58072
namespace FrSpecs.RustPreservation

namespace Before
set_option linter.unusedVariables false in
def allowedModel (a : Bool) (b : Bool) (unused : Bool) : Bool :=
  (a && (!b))
end Before

namespace After
set_option linter.unusedVariables false in
def permitsModel (a : Bool) (b : Bool) : Bool :=
  (a && (!b))
end After

-- fr:property model-comparison agent-authored
theorem preserves : ∀ (v0 : Bool) (v1 : Bool) (v2 : Bool), (Before.allowedModel v0 v1 v2) = (After.permitsModel v0 v1) := by
  -- fr:proof-begin preserves
  decide
  -- fr:proof-end preserves

end FrSpecs.RustPreservation
