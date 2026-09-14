namespace FrKernels.Surface

-- fr:spec src/project/surface_edit.rs::surface_value_size_allowed @ 129077e026079fce8a86c3b86a4eab9736c519177533f39ddbcaa570b5286cdd
-- fr:signature bytes: usize => bytes: Nat; return: bool => return: Bool
def surfaceValueSizeAllowed (bytes : Nat) : Bool :=
  decide (1 ≤ bytes ∧ bytes ≤ 256)

theorem surface_value_size_allowed_iff (bytes : Nat) :
    surfaceValueSizeAllowed bytes = true ↔ 1 ≤ bytes ∧ bytes ≤ 256 := by
  simp [surfaceValueSizeAllowed]

theorem accepted_surface_values_are_nonempty (bytes : Nat)
    (accepted : surfaceValueSizeAllowed bytes = true) :
    0 < bytes := by
  exact (surface_value_size_allowed_iff bytes).mp accepted |>.1

-- fr:spec src/project/surface_kernel.rs::style_literal_resolution @ f539b658f8f946f9eb431da2a3625f2d99ed865dbebda195946624c786d08bc9
-- fr:signature definition_count: usize => definitionCount: Nat; tailwind_context: bool => tailwindContext: Bool; return: usize => return: Nat
def styleLiteralResolution (definitionCount : Nat) (tailwindContext : Bool) : Nat :=
  if definitionCount = 1 then 0
  else if definitionCount > 1 then 1
  else if tailwindContext then 2
  else 3

theorem style_resolution_is_known (definitionCount : Nat) (tailwindContext : Bool) :
    styleLiteralResolution definitionCount tailwindContext ≤ 3 := by
  unfold styleLiteralResolution
  split
  · omega
  · split
    · omega
    · cases tailwindContext <;> decide

theorem unique_definition_resolves (tailwindContext : Bool) :
    styleLiteralResolution 1 tailwindContext = 0 := by
  simp [styleLiteralResolution]

theorem tailwind_only_classifies_without_definitions (definitionCount : Nat) :
    styleLiteralResolution definitionCount true = 2 ↔ definitionCount = 0 := by
  unfold styleLiteralResolution
  split
  · omega
  · split
    · omega
    · simp
      omega

-- fr:spec src/project/surface_kernel.rs::surface_items_emitted @ e11e05e31b7945129c007d7d92bf699c9ced0240544a88086e264b20c4b639c0
-- fr:signature total: usize => total: Nat; limit: usize => limit: Nat; return: usize => return: Nat
def itemsEmitted (total : Nat) (limit : Nat) : Nat := min total limit

-- fr:spec src/project/surface_kernel.rs::surface_items_omitted @ a911c8477f51e08e0937c6adc834662386346c882b1ae8ccdef1e1c091eba6aa
-- fr:signature total: usize => total: Nat; limit: usize => limit: Nat; return: usize => return: Nat
def itemsOmitted (total : Nat) (limit : Nat) : Nat := total - limit

theorem item_partition (total limit : Nat) :
    itemsEmitted total limit + itemsOmitted total limit = total := by
  unfold itemsEmitted itemsOmitted
  omega

theorem emitted_respects_limit (total limit : Nat) :
    itemsEmitted total limit ≤ limit := Nat.min_le_right _ _

theorem emitted_respects_total (total limit : Nat) :
    itemsEmitted total limit ≤ total := Nat.min_le_left _ _

end FrKernels.Surface
