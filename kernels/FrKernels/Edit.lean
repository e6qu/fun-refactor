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

def byteToCharIndex (source : List Char) (offset : Nat) : Option Nat :=
  go source offset 0
where
  go : List Char -> Nat -> Nat -> Option Nat
    | [], 0, index => some index
    | [], _ + 1, _ => none
    | _ :: _, 0, index => some index
    | character :: rest, offset + 1, index =>
      if character.utf8Size <= offset + 1 then
        go rest (offset + 1 - character.utf8Size) (index + 1)
      else
        none

def splice (source : String) (edit : Edit) : String :=
  match byteToCharIndex source.toList edit.start, byteToCharIndex source.toList edit.stop with
  | some start, some stop =>
    String.ofList <| source.toList.take start ++ edit.replacement.toList ++ source.toList.drop stop
  | _, _ => source

def within (source : String) (edit : Edit) : Bool :=
  edit.start <= edit.stop &&
    (byteToCharIndex source.toList edit.start).isSome &&
      (byteToCharIndex source.toList edit.stop).isSome

def disjoint : List Edit -> Bool
  | [] => true
  | [_] => true
  | left :: right :: tail => left.stop <= right.start && disjoint (right :: tail)

def valid (source : String) (edits : List Edit) : Bool :=
  edits.all (within source) && disjoint (order edits)

def apply (source : String) (edits : List Edit) : String :=
  (order edits).reverse.foldl splice source

-- fr:spec src/edit.rs::apply_to_string @ 3e192284
def applyChecked (source : String) (edits : List Edit) : Option String :=
  if valid source edits then some (apply source edits) else none

theorem splice_is_one_prefix_replacement_suffix (source : String) (edit : Edit)
    (start stop : Nat)
    (startAt : byteToCharIndex source.toList edit.start = some start)
    (stopAt : byteToCharIndex source.toList edit.stop = some stop) :
    splice source edit = String.ofList
      (source.toList.take start ++ edit.replacement.toList ++ source.toList.drop stop) := by
  simp [splice, startAt, stopAt]

theorem splice_keeps_the_prefix (source : String) (edit : Edit) (start stop : Nat)
    (startAt : byteToCharIndex source.toList edit.start = some start)
    (stopAt : byteToCharIndex source.toList edit.stop = some stop)
    (startIn : start ≤ source.toList.length) :
    (splice source edit).toList.take start = source.toList.take start := by
  rw [splice_is_one_prefix_replacement_suffix source edit start stop startAt stopAt]
  simp only [String.toList_ofList]
  have startInChars : start ≤ source.length := by simpa using startIn
  rw [List.take_append_of_le_length (by simp [List.length_take]; omega)]
  rw [List.take_append_of_le_length (by simp [List.length_take]; omega)]
  rw [List.take_take, Nat.min_self]

theorem no_edits_leave_the_source_alone (source : String) : apply source [] = source := by
  simp [apply, order]

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
example : valid "aé" [{ start := 1, stop := 2, replacement := "X" }] = false := by decide
example : valid "aé" [{ start := 1, stop := 3, replacement := "λ" }] = true := by decide

end FrKernels
