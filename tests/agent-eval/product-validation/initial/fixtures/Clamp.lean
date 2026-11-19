import Std

namespace Product

def clamp (lower upper value : Nat) : Nat :=
  if value < lower then lower else if upper < value then upper else value

theorem clamp_bounds (lower upper value : Nat) (ordered : lower ≤ upper) :
    lower ≤ clamp lower upper value ∧
    clamp lower upper value ≤ upper ∧
    clamp lower upper (clamp lower upper value) = clamp lower upper value := by
  -- fr:proof-begin clamp_bounds
  sorry
  -- fr:proof-end clamp_bounds

end Product
