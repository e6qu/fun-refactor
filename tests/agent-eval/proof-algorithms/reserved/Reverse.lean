import Std

namespace Transfer

def reverseInto {α : Type} (xs acc : List α) : List α :=
  match xs with
  | [] => acc
  | x :: rest => reverseInto rest (x :: acc)

theorem reverseInto_correct {α : Type} (xs acc : List α) :
    reverseInto xs acc = xs.reverse ++ acc := by
  -- fr:proof-begin reverseInto_correct
  -- fr:debt reverseInto_correct
  sorry
  -- fr:proof-end reverseInto_correct

end Transfer
