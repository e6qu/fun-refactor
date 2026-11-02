prelude
import Init.Data.Nat.Gcd

namespace Algorithm

theorem euclid_decreases (a b : Nat) (hb : b ≠ 0) : a % b < b := by
  exact Nat.mod_lt a (Nat.pos_of_ne_zero hb)

def euclid (a b : Nat) : Nat :=
  if b = 0 then a else euclid b (a % b)
termination_by b
decreasing_by
  apply euclid_decreases
  assumption

theorem euclid_correct (a b : Nat) : euclid a b = Nat.gcd a b := by
  have main : ∀ k a, euclid a k = Nat.gcd a k :=
    fun k => Nat.strongRecOn (motive := fun j => ∀ a, euclid a j = Nat.gcd a j) k
      (fun j ih a => by
        rw [euclid]
        split
        · rename_i hb
          simp [hb]
        · rename_i hb
          rw [ih (a % j) (euclid_decreases a j hb)]
          rw [Nat.gcd_comm a j, Nat.gcd_rec j a]
          exact Nat.gcd_comm _ _)
  exact main b a

theorem afterEuclidLambda : 2 + 2 = 4 := by
  rfl

end Algorithm

def afterAlgorithmLambda : Nat := 7
