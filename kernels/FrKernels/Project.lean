namespace FrKernels.Project

-- fr:spec src/project.rs::path_confidence @ b5a8549e
-- fr:signature edges: &[Confidence] => edges: List Nat; return: Confidence => return: Nat
def pathConfidence (edges : List Nat) : Nat := edges.foldr max 0

theorem path_confidence_empty : pathConfidence [] = 0 := rfl

theorem path_confidence_cannot_strengthen (edges : List Nat) (edge : Nat)
    (member : edge ∈ edges) : edge ≤ pathConfidence edges := by
  induction edges with
  | nil => simp at member
  | cons head tail ih =>
    change edge ≤ max head (pathConfidence tail)
    rcases List.mem_cons.mp member with same | rest
    · subst edge
      exact Nat.le_max_left _ _
    · exact Nat.le_trans (ih rest) (Nat.le_max_right _ _)

theorem path_confidence_stays_in_tiers (edges : List Nat) (ceiling : Nat)
    (bounded : ∀ edge ∈ edges, edge ≤ ceiling) : pathConfidence edges ≤ ceiling := by
  induction edges with
  | nil => exact Nat.zero_le _
  | cons head tail ih =>
    change max head (pathConfidence tail) ≤ ceiling
    exact Nat.max_le.mpr ⟨bounded head (by simp), ih (fun edge member => bounded edge (by simp [member]))⟩

-- fr:spec src/project.rs::page_length @ b4a90c73
-- fr:signature total: usize => total: Nat; start: usize => start: Nat; limit: usize => limit: Nat; return: usize => return: Nat
def pageLength (total : Nat) (start : Nat) (limit : Nat) : Nat :=
  min (total - start) limit

theorem page_respects_limit (total start limit : Nat) : pageLength total start limit ≤ limit := by
  exact Nat.min_le_right _ _

theorem page_stays_in_result (total start limit : Nat) (valid : start ≤ total) :
    start + pageLength total start limit ≤ total := by
  unfold pageLength
  omega

theorem page_advances (total start limit : Nat) (remaining : start < total) (positive : 0 < limit) :
    start < start + pageLength total start limit := by
  unfold pageLength
  omega

theorem page_and_remaining_partition (total start limit : Nat) (valid : start ≤ total) :
    pageLength total start limit + (total - (start + pageLength total start limit)) = total - start := by
  unfold pageLength
  omega

theorem beyond_end_is_empty (total start limit : Nat) (ended : total ≤ start) :
    pageLength total start limit = 0 := by
  unfold pageLength
  omega

-- fr:spec src/project.rs::workspace_pattern_matches @ 3166b1cc
-- fr:signature pattern: &[String] => pattern: List String; path: &[String] => path: List String; return: bool => return: Bool
def workspacePatternMatches (pattern : List String) (path : List String) : Bool :=
  match pattern, path with
  | [], [] => true
  | p :: ps, part :: parts => (p == "*" || p == part) && workspacePatternMatches ps parts
  | _, _ => false

theorem matches_preserve_depth (pattern path : List String)
    (h : workspacePatternMatches pattern path = true) : pattern.length = path.length := by
  induction pattern generalizing path with
  | nil => cases path <;> simp_all [workspacePatternMatches]
  | cons p ps ih =>
    cases path with
    | nil => simp [workspacePatternMatches] at h
    | cons part parts =>
      simp only [workspacePatternMatches, Bool.and_eq_true] at h
      simpa using congrArg Nat.succ (ih parts h.2)

theorem different_depth_refuses (pattern path : List String)
    (h : pattern.length ≠ path.length) : workspacePatternMatches pattern path = false := by
  cases result : workspacePatternMatches pattern path with
  | false => rfl
  | true => exact False.elim (h (matches_preserve_depth pattern path result))

theorem literal_path_matches_itself (path : List String) : workspacePatternMatches path path = true := by
  induction path with
  | nil => rfl
  | cons part parts ih => simp [workspacePatternMatches, ih]

theorem matched_head_is_literal_or_star (p part : String) (ps parts : List String)
    (h : workspacePatternMatches (p :: ps) (part :: parts) = true) : p = "*" ∨ p = part := by
  simp only [workspacePatternMatches, Bool.and_eq_true] at h
  simpa using h.1

end FrKernels.Project
