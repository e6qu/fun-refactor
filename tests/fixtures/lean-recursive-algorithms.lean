import Std

def euclid (a b : Nat) : Nat :=
  if b = 0 then a else euclid b (a % b)
termination_by b
decreasing_by
  apply Nat.mod_lt
  exact Nat.pos_of_ne_zero (by assumption)

theorem afterEuclid : euclid 48 18 = 6 := by
  simp [euclid]

def reverseInto {α : Type} (xs acc : List α) : List α :=
  match xs with
  | [] => acc
  | x :: rest => reverseInto rest (x :: acc)
termination_by structural xs

theorem afterReverse : reverseInto [1, 2, 3] [9] = [3, 2, 1, 9] := by
  rfl
