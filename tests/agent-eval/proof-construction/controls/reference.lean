intro h
rcases h with ⟨ha, hb⟩
change a ≤ a * b
have hmul : a * 1 ≤ a * b := Int.mul_le_mul_of_nonneg_left hb ha
simpa only [Int.mul_one] using hmul
