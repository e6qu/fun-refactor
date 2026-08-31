namespace FrKernels

structure Edit where
  start : Nat
  stop : Nat
  replacement : String
  deriving Repr

def before (left right : Edit) : Bool :=
  left.start < right.start || left.start == right.start && left.stop <= right.stop

def insert (edit : Edit) : List Edit -> List Edit
  | [] => [edit]
  | head :: tail => if before edit head then edit :: head :: tail else head :: insert edit tail

def order : List Edit -> List Edit
  | [] => []
  | head :: tail => insert head (order tail)

def splice (source : List Char) (edit : Edit) : List Char :=
  source.take edit.start ++ edit.replacement.toList ++ source.drop edit.stop

def within (source : String) (edit : Edit) : Bool :=
  edit.start <= edit.stop && edit.stop <= source.length

def disjoint : List Edit -> Bool
  | [] => true
  | [_] => true
  | left :: right :: tail => left.stop <= right.start && disjoint (right :: tail)

def valid (source : String) (edits : List Edit) : Bool :=
  edits.all (within source) && disjoint (order edits)

def apply (source : String) (edits : List Edit) : String :=
  String.ofList <| (order edits).reverse.foldl splice source.toList

def applyChecked (source : String) (edits : List Edit) : Option String :=
  if valid source edits then some (apply source edits) else none

theorem splice_is_one_prefix_replacement_suffix (source : List Char) (edit : Edit) :
    splice source edit = source.take edit.start ++ edit.replacement.toList ++ source.drop edit.stop := by
  rfl

theorem splice_keeps_the_prefix (source : List Char) (edit : Edit)
    (inside : edit.start ≤ source.length) :
    (splice source edit).take edit.start = source.take edit.start := by
  rw [splice, List.take_append_of_le_length (by simp [List.length_take, inside])]
  rw [List.take_append_of_le_length (by simp [List.length_take, inside])]
  rw [List.take_take, Nat.min_self]

theorem no_edits_leave_the_source_alone (source : String) : apply source [] = source := by
  change String.ofList source.toList = source
  exact String.ofList_toList

theorem rejected_plan_has_no_result (source : String) (edits : List Edit)
    (invalid : valid source edits = false) : applyChecked source edits = none := by
  simp [applyChecked, invalid]

theorem accepted_plan_has_one_result (source : String) (edits : List Edit)
    (accepted : valid source edits = true) : applyChecked source edits = some (apply source edits) := by
  simp [applyChecked, accepted]

example : apply "abc" [{ start := 1, stop := 2, replacement := "XY" }] = "aXYc" := by decide
example : apply "abcd" [
  { start := 3, stop := 4, replacement := "Z" },
  { start := 0, stop := 1, replacement := "W" }
] = "WbcZ" := by decide
example : valid "abcdef" [
  { start := 0, stop := 3, replacement := "X" },
  { start := 3, stop := 6, replacement := "Y" }
] = true := by decide
example : valid "abcdef" [
  { start := 0, stop := 3, replacement := "X" },
  { start := 2, stop := 5, replacement := "Y" }
] = false := by decide

end FrKernels
