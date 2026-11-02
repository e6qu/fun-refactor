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
