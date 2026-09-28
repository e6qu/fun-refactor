-- fr:comparison cacdc3509fcc918d42faf46a00d800a71a8e5a53733c87f2d9f04000204be503
namespace FrSpecs.Restriction

namespace Before
set_option linter.unusedVariables false in
def allowedModel (a : Bool) (b : Bool) (unused : Bool) : Bool :=
  (a && (!b))
end Before

namespace After
set_option linter.unusedVariables false in
def TranslationPyRestrictedModel (a : Bool) (b : Bool) (extra : Bool) : Bool :=
  ((a && (!b)) && extra)
end After

-- fr:property model-comparison agent-authored
theorem preserves : ∀ (v0 : Bool) (v1 : Bool) (v2 : Bool), (After.TranslationPyRestrictedModel v0 v1 v2) = true → (Before.allowedModel v0 v1 v2) = true := by
  -- fr:proof-begin preserves
  decide
  -- fr:proof-end preserves

end FrSpecs.Restriction
