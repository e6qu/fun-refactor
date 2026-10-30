-- fr:comparison db2e1dee686e284097a74943bd274e24119a33f82d177e85d44f69e206571a90
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
