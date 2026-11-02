induction b using Nat.strongRecOn generalizing a with
| ind b ih =>
  rw [euclid]
  split
  · rename_i hb
    simp [hb]
  · rename_i hb
    rw [ih (a % b) (euclid_decreases a b hb)]
    rw [Nat.gcd_comm a b, Nat.gcd_rec b a]
    exact Nat.gcd_comm _ _
