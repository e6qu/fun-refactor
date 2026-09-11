import FrKernels.Source

namespace FrKernels.Author

open FrKernels.Source

-- fr:spec src/project.rs::reviewed_plan_basis_allowed @ 5a1dfed6336ba17fa1424b0f3d5ae5f7b11841d018c7c0718aa9ac9d0f1d3620
-- fr:signature complete: bool => complete: Bool; supplied: bool => supplied: Bool; matches: bool => identityMatches: Bool; return: bool => return: Bool
def reviewedPlanBasisAllowed (complete : Bool) (supplied : Bool) (identityMatches : Bool) : Bool :=
  !supplied || complete && identityMatches

theorem reviewed_plan_without_basis_is_allowed (complete identityMatches : Bool) :
    reviewedPlanBasisAllowed complete false identityMatches = true := by
  cases complete <;> cases identityMatches <;> decide

theorem supplied_plan_basis_allowed_iff_complete_and_matching (complete identityMatches : Bool) :
    reviewedPlanBasisAllowed complete true identityMatches = true ↔
      complete = true ∧ identityMatches = true := by
  cases complete <;> cases identityMatches <;> decide

theorem incomplete_supplied_plan_is_rejected (identityMatches : Bool) :
    reviewedPlanBasisAllowed false true identityMatches = false := by
  cases identityMatches <;> decide

theorem conflicting_supplied_plan_is_rejected (complete : Bool) :
    reviewedPlanBasisAllowed complete true false = false := by
  cases complete <;> decide

def isIndent (c : Char) : Bool := c == ' ' || c == '\t' || c == '\r'

def insertionLine : List Char → Nat
  | [] => 0
  | c :: rest =>
    if c == '\n' && rest.all isIndent then c.utf8Size
    else c.utf8Size + insertionLine rest

theorem line_in_bounds (chars : List Char) : insertionLine chars ≤ byteLength chars := by
  induction chars with
  | nil => simp [insertionLine, byteLength]
  | cons c rest ih =>
    simp only [insertionLine, byteLength, List.map_cons, List.sum_cons]
    simp only [byteLength] at ih
    split <;> omega

theorem line_is_boundary (chars : List Char) : insertionLine chars ∈ boundaries chars := by
  induction chars with
  | nil => simp [insertionLine, boundaries]
  | cons c rest ih =>
    simp only [insertionLine]
    split
    · exact List.mem_cons_of_mem _ (List.mem_map.mpr
        ⟨0, zero_is_boundary rest, Nat.add_zero c.utf8Size⟩)
    · exact List.mem_cons_of_mem _ (List.mem_map.mpr ⟨_, ih, rfl⟩)

theorem line_has_indent_suffix (chars : List Char) :
    ∃ before after, chars = before ++ after ∧ insertionLine chars = byteLength before ∧ after.all isIndent = true := by
  induction chars with
  | nil => exact ⟨[], [], rfl, rfl, rfl⟩
  | cons c rest ih =>
    simp only [insertionLine]
    split
    · rename_i h
      simp only [Bool.and_eq_true] at h
      exact ⟨[c], rest, rfl, by simp [byteLength], h.2⟩
    · obtain ⟨before, after, same, offset, clean⟩ := ih
      exact ⟨c :: before, after, by simp [same], by simpa only [byteLength, List.map_cons, List.sum_cons] using congrArg (c.utf8Size + ·) offset, clean⟩

theorem line_after_newline (before indent : List Char) (clean : indent.all isIndent = true) :
    insertionLine (before ++ '\n' :: indent) = byteLength before + 1 := by
  induction before with
  | nil => simp [insertionLine, clean, byteLength, Char.utf8Size]
  | cons c rest ih => simp [insertionLine, List.all_append, isIndent, ih, byteLength, Nat.add_assoc]

theorem line_after_content (before : List Char) (c : Char)
    (notNewline : (c == '\n') = false) (notIndent : isIndent c = false) :
    insertionLine (before ++ [c]) = byteLength before + c.utf8Size := by
  induction before with
  | nil => simp [insertionLine, notNewline, byteLength]
  | cons head rest ih => simp [insertionLine, List.all_append, notIndent, ih, byteLength, Nat.add_assoc]

-- fr:spec src/project.rs::declaration_insertion_offset @ 97b9aa9b57f5165a89d93c9fa994670d801b77a7ae49c0bbc3acb9b92caa1e92
-- fr:signature prefix: &str => text: String; body_start: usize => bodyStart: Nat; return: usize => return: Nat
def declarationInsertionOffset (text : String) (bodyStart : Nat) : Nat :=
  let line := insertionLine text.toList
  if bodyStart < line then line else byteLength text.toList

theorem offset_in_bounds (text : String) (bodyStart : Nat) :
    declarationInsertionOffset text bodyStart ≤ byteLength text.toList := by
  dsimp only [declarationInsertionOffset]
  split
  · exact line_in_bounds _
  · exact Nat.le_refl _

theorem offset_is_boundary (text : String) (bodyStart : Nat) :
    declarationInsertionOffset text bodyStart ∈ boundaries text.toList := by
  dsimp only [declarationInsertionOffset]
  split
  · exact line_is_boundary _
  · exact end_is_boundary _

theorem offset_after_opening (text : String) (bodyStart : Nat)
    (inside : bodyStart < byteLength text.toList) : bodyStart < declarationInsertionOffset text bodyStart := by
  dsimp only [declarationInsertionOffset]
  split <;> assumption

theorem offset_uses_inner_line (text : String) (bodyStart : Nat)
    (inside : bodyStart < insertionLine text.toList) :
    declarationInsertionOffset text bodyStart = insertionLine text.toList := by
  simp [declarationInsertionOffset, inside]

theorem offset_at_close_when_line_outside (text : String) (bodyStart : Nat)
    (outside : insertionLine text.toList ≤ bodyStart) :
    declarationInsertionOffset text bodyStart = byteLength text.toList := by
  simp [declarationInsertionOffset, Nat.not_lt.mpr outside]

theorem offset_keeps_inline_close (text : String) (bodyStart : Nat)
    (inline : insertionLine text.toList = byteLength text.toList) :
    declarationInsertionOffset text bodyStart = byteLength text.toList := by
  simp [declarationInsertionOffset, inline]

theorem offset_has_indent_suffix (text : String) (bodyStart : Nat) :
    ∃ before after, text.toList = before ++ after ∧
      declarationInsertionOffset text bodyStart = byteLength before ∧ after.all isIndent = true := by
  dsimp only [declarationInsertionOffset]
  split
  · exact line_has_indent_suffix _
  · exact ⟨text.toList, [], by simp, rfl, rfl⟩

theorem offset_empty (bodyStart : Nat) : declarationInsertionOffset "" bodyStart = 0 := by
  simp [declarationInsertionOffset, insertionLine, byteLength]

theorem offset_preserves_indent (before indent : List Char) (bodyStart : Nat)
    (clean : indent.all isIndent = true) (inside : bodyStart < byteLength before + 1) :
    declarationInsertionOffset (String.ofList (before ++ '\n' :: indent)) bodyStart = byteLength before + 1 := by
  simp [declarationInsertionOffset, line_after_newline before indent clean, inside]

theorem offset_at_close_after_content (before : List Char) (c : Char) (bodyStart : Nat)
    (notNewline : (c == '\n') = false) (notIndent : isIndent c = false) :
    declarationInsertionOffset (String.ofList (before ++ [c])) bodyStart = byteLength (before ++ [c]) := by
  simp [declarationInsertionOffset, line_after_content before c notNewline notIndent, byteLength]

-- fr:spec src/project.rs::author_selection_conflict @ 4edcb6cfdd7e1ae607a8188fc61eced535d0180442d1c012634e3e5e6561e2a4
-- fr:signature left_start: usize => leftStart: Nat; left_end: usize => leftEnd: Nat; right_start: usize => rightStart: Nat; right_end: usize => rightEnd: Nat; return: bool => return: Bool
def selectionConflict (leftStart : Nat) (leftEnd : Nat) (rightStart : Nat) (rightEnd : Nat) : Bool :=
  decide ((leftStart < rightEnd ∧ rightStart < leftEnd) ∨
    (leftStart = leftEnd ∧ rightStart ≤ leftStart ∧ leftStart ≤ rightEnd) ∨
    (rightStart = rightEnd ∧ leftStart ≤ rightStart ∧ rightStart ≤ leftEnd))

theorem selection_conflict_is_symmetric (leftStart leftEnd rightStart rightEnd : Nat) :
    selectionConflict leftStart leftEnd rightStart rightEnd =
      selectionConflict rightStart rightEnd leftStart leftEnd := by
  simp only [selectionConflict, decide_eq_decide]
  omega

theorem adjacent_nonempty_selections_do_not_conflict (start boundary stop : Nat)
    (leftNonempty : start < boundary) (rightNonempty : boundary < stop) :
    selectionConflict start boundary boundary stop = false := by
  simp [selectionConflict]
  omega

theorem overlapping_nonempty_selections_conflict (leftStart rightStart leftEnd rightEnd : Nat)
    (leftFirst : leftStart ≤ rightStart) (overlap : rightStart < leftEnd)
    (rightValid : leftEnd ≤ rightEnd) :
    selectionConflict leftStart leftEnd rightStart rightEnd = true := by
  simp [selectionConflict]
  omega

theorem insertion_at_left_boundary_conflicts (start stop : Nat) (valid : start ≤ stop) :
    selectionConflict start start start stop = true := by
  simp [selectionConflict]
  omega

theorem insertion_at_right_boundary_conflicts (start stop : Nat) (valid : start ≤ stop) :
    selectionConflict stop stop start stop = true := by
  simp [selectionConflict]
  omega

theorem distinct_separated_insertions_do_not_conflict (left right : Nat) (apart : left < right) :
    selectionConflict left left right right = false := by
  simp [selectionConflict]
  omega

end FrKernels.Author
