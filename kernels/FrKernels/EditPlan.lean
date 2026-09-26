import FrKernels.Edit
namespace FrKernels.EditPlan

def spliceChars (source : List Char) (edit : Edit) : List Char :=
  match byteToCharIndex source edit.start, byteToCharIndex source edit.stop with
  | some start, some stop => source.take start ++ edit.replacement.toList ++ source.drop stop
  | _, _ => source

def withinChars (source : List Char) (edit : Edit) : Bool :=
  edit.start <= edit.stop &&
      (byteToCharIndex source edit.start).isSome &&
      (byteToCharIndex source edit.stop).isSome

def checkedChars (source : List Char) (edits : List Edit) : Option (List Char) :=
  if edits.all (withinChars source) && disjoint (order edits) then
    some ((order edits).reverse.foldl spliceChars source)
  else none

theorem splice_ofList (source : List Char) (edit : Edit) :
    splice (String.ofList source) edit = String.ofList (spliceChars source edit) := by
  cases hs : byteToCharIndex source edit.start <;>
    cases he : byteToCharIndex source edit.stop <;>
    simp only [splice, spliceChars, String.toList_ofList, hs, he]

theorem within_ofList (source : List Char) :
    within (String.ofList source) = withinChars source := by
  funext edit
  simp only [within, withinChars, String.toList_ofList]

theorem fold_splice_ofList (edits : List Edit) (source : List Char) :
    edits.foldl splice (String.ofList source) =
      String.ofList (edits.foldl spliceChars source) := by
  induction edits generalizing source with
  | nil => rfl
  | cons edit rest ih => simp only [List.foldl_cons, splice_ofList, ih]

theorem applyChecked_ofList (source : List Char) (edits : List Edit) :
    applyChecked (String.ofList source) edits =
      (checkedChars source edits).map String.ofList := by
  simp only [applyChecked, valid, within_ofList, checkedChars]
  split <;> simp only [apply, fold_splice_ofList, Option.map_some, Option.map_none]

theorem checked_from_chars (source expected : List Char) (edits : List Edit)
    (checked : checkedChars source edits = some expected) :
    applyChecked (String.ofList source) edits = some (String.ofList expected) := by
  rw [applyChecked_ofList, checked]
  rfl
end FrKernels.EditPlan
