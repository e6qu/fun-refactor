import Std

namespace Algorithm

theorem euclid_decreases (a b : Nat) (hb : b ≠ 0) : a % b < b := by
  -- fr:proof-begin euclid_decreases
  -- fr:debt euclid_decreases
  sorry
  -- fr:proof-end euclid_decreases

def euclid (a b : Nat) : Nat :=
  if b = 0 then a else euclid b (a % b)
termination_by b
decreasing_by
  apply euclid_decreases
  assumption

theorem euclid_correct (a b : Nat) : euclid a b = Nat.gcd a b := by
  -- fr:proof-begin euclid_correct
  -- fr:debt euclid_correct
  sorry
  -- fr:proof-end euclid_correct

end Algorithm
