import FrKernels.Project

namespace FrKernels.Source

def byteLength (chars : List Char) : Nat := (chars.map Char.utf8Size).sum

def boundaries : List Char → List Nat
  | [] => [0]
  | head :: tail => 0 :: (boundaries tail).map (head.utf8Size + ·)

theorem zero_is_boundary (chars : List Char) : 0 ∈ boundaries chars := by
  cases chars <;> simp [boundaries]

theorem boundary_in_source (chars : List Char) (offset : Nat)
    (valid : offset ∈ boundaries chars) : offset ≤ byteLength chars := by
  induction chars generalizing offset with
  | nil => simpa [boundaries, byteLength] using valid
  | cons head tail ih =>
    simp only [boundaries, List.mem_cons, List.mem_map] at valid
    rcases valid with rfl | ⟨n, member, rfl⟩
    · exact Nat.zero_le _
    · simpa [byteLength] using Nat.add_le_add_left (ih n member) head.utf8Size

theorem end_is_boundary (chars : List Char) : byteLength chars ∈ boundaries chars := by
  induction chars with
  | nil => simp [byteLength, boundaries]
  | cons head tail ih =>
    simp only [byteLength, List.map_cons, List.sum_cons, boundaries]
    exact List.mem_cons_of_mem _ (List.mem_map.mpr ⟨byteLength tail, ih, rfl⟩)

def floorBoundary (valid : Nat → Bool) : Nat → Nat
  | 0 => 0
  | n + 1 => if valid (n + 1) then n + 1 else floorBoundary valid n

theorem floor_bounded (valid : Nat → Bool) (limit : Nat) :
    floorBoundary valid limit ≤ limit := by
  induction limit with
  | zero => simp [floorBoundary]
  | succ n ih => simp only [floorBoundary]; split <;> omega

theorem floor_is_boundary (valid : Nat → Bool) (limit : Nat) (zero : valid 0 = true) :
    valid (floorBoundary valid limit) = true := by
  induction limit with
  | zero => exact zero
  | succ n ih => simp only [floorBoundary]; split <;> assumption

theorem floor_is_greatest (valid : Nat → Bool) (limit candidate : Nat)
    (within : candidate ≤ limit) (boundary : valid candidate = true) :
    candidate ≤ floorBoundary valid limit := by
  induction limit with
  | zero => simpa [floorBoundary] using within
  | succ n ih =>
    simp only [floorBoundary]
    split
    · exact within
    · have : candidate ≠ n + 1 := by intro same; subst candidate; contradiction
      exact ih (by omega)

-- fr:spec src/project.rs::source_slice_length @ bac9157178522dc815a07072a199cb80bd6f3d603984519ef6f110f8248d4c5e
-- fr:signature text: &str => text: String; offset: usize => offset: Nat; bytes: usize => bytes: Nat; return: Option<usize> => return: Option Nat
def sourceSliceLength (text : String) (offset : Nat) (bytes : Nat) : Option Nat :=
  if offset ∈ boundaries text.toList then
    some (floorBoundary (fun n => decide (offset + n ∈ boundaries text.toList))
      (FrKernels.Project.pageLength (byteLength text.toList) offset bytes))
  else none

theorem slice_refuses_invalid_offset (text : String) (offset bytes : Nat)
    (invalid : offset ∉ boundaries text.toList) : sourceSliceLength text offset bytes = none := by
  simp [sourceSliceLength, invalid]

theorem slice_accepts_iff_boundary (text : String) (offset bytes : Nat) :
    (sourceSliceLength text offset bytes).isSome = true ↔ offset ∈ boundaries text.toList := by
  simp [sourceSliceLength]

theorem slice_bounds (text : String) (offset bytes length : Nat)
    (accepted : sourceSliceLength text offset bytes = some length) :
    offset ∈ boundaries text.toList ∧ offset + length ∈ boundaries text.toList ∧
      length ≤ bytes ∧ offset + length ≤ byteLength text.toList := by
  unfold sourceSliceLength at accepted
  split at accepted
  · rename_i valid
    cases accepted
    have start := boundary_in_source text.toList offset valid
    have bound := floor_bounded (fun n => decide (offset + n ∈ boundaries text.toList))
      (FrKernels.Project.pageLength (byteLength text.toList) offset bytes)
    have stop := floor_is_boundary (fun n => decide (offset + n ∈ boundaries text.toList))
      (FrKernels.Project.pageLength (byteLength text.toList) offset bytes) (by simpa using valid)
    refine ⟨valid, by simpa using stop, ?_, ?_⟩
    · exact Nat.le_trans bound (FrKernels.Project.page_respects_limit _ _ _)
    · exact Nat.le_trans (Nat.add_le_add_left bound offset)
        (FrKernels.Project.page_stays_in_result _ _ _ start)
  · contradiction

theorem slice_is_maximal (text : String) (offset bytes length stop : Nat)
    (accepted : sourceSliceLength text offset bytes = some length)
    (afterStart : offset ≤ stop) (withinBudget : stop - offset ≤ bytes)
    (boundary : stop ∈ boundaries text.toList) : stop ≤ offset + length := by
  unfold sourceSliceLength at accepted
  split at accepted
  · cases accepted
    have stopIn := boundary_in_source text.toList stop boundary
    have greatest := floor_is_greatest (fun n => decide (offset + n ∈ boundaries text.toList))
      (FrKernels.Project.pageLength (byteLength text.toList) offset bytes) (stop - offset)
      (by unfold FrKernels.Project.pageLength; omega) (by simpa [Nat.add_sub_cancel' afterStart] using boundary)
    omega
  · contradiction

theorem slice_and_remaining_partition (text : String) (offset bytes length : Nat)
    (accepted : sourceSliceLength text offset bytes = some length) :
    length + (byteLength text.toList - (offset + length)) = byteLength text.toList - offset := by
  have := slice_bounds text offset bytes length accepted
  omega

theorem slice_zero_budget (text : String) (offset : Nat)
    (valid : offset ∈ boundaries text.toList) : sourceSliceLength text offset 0 = some 0 := by
  simp [sourceSliceLength, valid, FrKernels.Project.pageLength, floorBoundary]

theorem slice_finishes_when_budget_fits (text : String) (offset bytes length : Nat)
    (accepted : sourceSliceLength text offset bytes = some length)
    (fits : byteLength text.toList - offset ≤ bytes) : offset + length = byteLength text.toList := by
  have bounds := slice_bounds text offset bytes length accepted
  have greatest := slice_is_maximal text offset bytes length (byteLength text.toList) accepted
    (boundary_in_source text.toList offset bounds.1) fits (end_is_boundary text.toList)
  omega

theorem slice_advances_to_available_boundary (text : String) (offset bytes length stop : Nat)
    (accepted : sourceSliceLength text offset bytes = some length)
    (later : offset < stop) (fits : stop - offset ≤ bytes)
    (boundary : stop ∈ boundaries text.toList) : 0 < length := by
  have := slice_is_maximal text offset bytes length stop accepted (by omega) fits boundary
  omega

def sourcePageLengths : List String → Nat → List Nat
  | [], _ => []
  | text :: rest, budget =>
    let length := (sourceSliceLength text 0 budget).getD 0
    length :: sourcePageLengths rest (budget - length)

theorem prefix_length_bounded (text : String) (budget : Nat) :
    (sourceSliceLength text 0 budget).getD 0 ≤ budget := by
  cases result : sourceSliceLength text 0 budget with
  | none => simp
  | some length => exact (slice_bounds text 0 budget length result).2.2.1

theorem page_preserves_rows (texts : List String) (budget : Nat) :
    (sourcePageLengths texts budget).length = texts.length := by
  induction texts generalizing budget with
  | nil => rfl
  | cons text rest ih => simp [sourcePageLengths, ih]

theorem page_respects_shared_budget (texts : List String) (budget : Nat) :
    (sourcePageLengths texts budget).sum ≤ budget := by
  induction texts generalizing budget with
  | nil => simp [sourcePageLengths]
  | cons text rest ih =>
    simp only [sourcePageLengths, List.sum_cons]
    have first := prefix_length_bounded text budget
    have tail := ih (budget - (sourceSliceLength text 0 budget).getD 0)
    omega

theorem page_budget_partition (texts : List String) (budget : Nat) :
    (sourcePageLengths texts budget).sum + (budget - (sourcePageLengths texts budget).sum) = budget := by
  have := page_respects_shared_budget texts budget
  omega

theorem page_zero_budget (texts : List String) : sourcePageLengths texts 0 = texts.map (fun _ => 0) := by
  induction texts with
  | nil => rfl
  | cons text rest ih =>
    simp [sourcePageLengths, slice_zero_budget text 0 (zero_is_boundary text.toList), ih]

end FrKernels.Source
