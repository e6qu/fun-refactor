-- fr:comparison dcbcb17769a4deab65700a0103d04f0e3acd1c9632aec7b59ee26072246e0041
namespace FrSpecs.Translation

namespace Before
set_option linter.unusedVariables false in
def allowedModel (a : Bool) (b : Bool) (unused : Bool) : Bool :=
  (a && (!b))
end Before

namespace After
set_option linter.unusedVariables false in
def TranslationPyPermitsModel (b : Bool) (a : Bool) : Bool :=
  ((!b) && a)
end After

-- fr:property model-comparison agent-authored
theorem preserves : ∀ (v0 : Bool) (v1 : Bool) (v2 : Bool), (Before.allowedModel v0 v1 v2) = (After.TranslationPyPermitsModel v1 v0) := by
  -- fr:proof-begin preserves
  decide
  -- fr:proof-end preserves

end FrSpecs.Translation
