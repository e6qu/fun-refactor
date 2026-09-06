import Init.Data.List.Sort.Lemmas

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

def canonicalMembers (members : List Nat) : List Nat :=
  (members.foldr List.insert []).mergeSort (· ≤ ·)

theorem canonical_membership (members : List Nat) (node : Nat) :
    node ∈ canonicalMembers members ↔ node ∈ members := by
  simp only [canonicalMembers, List.mem_mergeSort]
  induction members with
  | nil => simp
  | cons head tail ih => simp [ih]

-- fr:spec src/project.rs::workspace_membership_step @ 5436711f
-- fr:signature members: &[usize] => members: List Nat; edges: &[(usize, usize)] => edges: List (Nat × Nat); return: Vec<usize> => return: List Nat
def workspaceMembershipStep (members : List Nat) (edges : List (Nat × Nat)) : List Nat :=
  canonicalMembers (members ++ (edges.filter (fun edge => edge.1 ∈ members)).map Prod.snd)

theorem membership_step_iff (members : List Nat) (edges : List (Nat × Nat)) (node : Nat) :
    node ∈ workspaceMembershipStep members edges ↔
      node ∈ members ∨ ∃ source, (source, node) ∈ edges ∧ source ∈ members := by
  simp [workspaceMembershipStep, canonical_membership, List.mem_map, List.mem_filter,
    Prod.exists, and_assoc]

theorem membership_step_preserves (members : List Nat) (edges : List (Nat × Nat))
    (node : Nat) (present : node ∈ members) : node ∈ workspaceMembershipStep members edges :=
  (membership_step_iff members edges node).mpr (Or.inl present)

theorem membership_step_monotone (smaller larger : List Nat) (edges : List (Nat × Nat))
    (included : ∀ node ∈ smaller, node ∈ larger) :
    ∀ node ∈ workspaceMembershipStep smaller edges, node ∈ workspaceMembershipStep larger edges := by
  intro node present
  apply (membership_step_iff larger edges node).mpr
  rcases (membership_step_iff smaller edges node).mp present with old | ⟨source, edge, known⟩
  · exact Or.inl (included node old)
  · exact Or.inr ⟨source, edge, included source known⟩

def membershipRounds (seeds : List Nat) (edges : List (Nat × Nat)) : Nat → List Nat
  | 0 => canonicalMembers seeds
  | rounds + 1 => workspaceMembershipStep (membershipRounds seeds edges rounds) edges

inductive MemberReachable (seeds : List Nat) (edges : List (Nat × Nat)) : Nat → Prop
  | seed {node} : node ∈ seeds → MemberReachable seeds edges node
  | edge {source target} : MemberReachable seeds edges source →
      (source, target) ∈ edges → MemberReachable seeds edges target

theorem membership_rounds_sound (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat) :
    ∀ node ∈ membershipRounds seeds edges rounds, MemberReachable seeds edges node := by
  induction rounds with
  | zero =>
    intro node present
    exact .seed ((canonical_membership seeds node).mp present)
  | succ rounds ih =>
    intro node present
    rcases (membership_step_iff _ edges node).mp present with old | ⟨source, edge, known⟩
    · exact ih node old
    · exact .edge (ih source known) edge

theorem membership_rounds_preserve_seeds (seeds : List Nat) (edges : List (Nat × Nat))
    (rounds node : Nat) (seed : node ∈ seeds) : node ∈ membershipRounds seeds edges rounds := by
  induction rounds with
  | zero => exact (canonical_membership seeds node).mpr seed
  | succ rounds ih => exact membership_step_preserves _ edges node ih

theorem reachable_in_closed_superset (seeds : List Nat) (edges : List (Nat × Nat))
    (allowed : Nat → Prop) (includes : ∀ node ∈ seeds, allowed node)
    (closed : ∀ source target, allowed source → (source, target) ∈ edges → allowed target)
    (node : Nat) (reachable : MemberReachable seeds edges node) : allowed node := by
  induction reachable with
  | seed present => exact includes _ present
  | edge _ edge ih => exact closed _ _ ih edge

theorem membership_rounds_stay_in_closed_superset (seeds : List Nat) (edges : List (Nat × Nat))
    (allowed : Nat → Prop) (includes : ∀ node ∈ seeds, allowed node)
    (closed : ∀ source target, allowed source → (source, target) ∈ edges → allowed target)
    (rounds node : Nat) (present : node ∈ membershipRounds seeds edges rounds) : allowed node :=
  reachable_in_closed_superset seeds edges allowed includes closed node
    (membership_rounds_sound seeds edges rounds node present)

theorem stabilized_membership_is_exact (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat)
    (stable : workspaceMembershipStep (membershipRounds seeds edges rounds) edges =
      membershipRounds seeds edges rounds) (node : Nat) :
    node ∈ membershipRounds seeds edges rounds ↔ MemberReachable seeds edges node := by
  constructor
  · exact membership_rounds_sound seeds edges rounds node
  · apply reachable_in_closed_superset seeds edges (fun n => n ∈ membershipRounds seeds edges rounds)
    · exact fun n h => membership_rounds_preserve_seeds seeds edges rounds n h
    · intro source target known edge
      rw [← stable]
      exact (membership_step_iff _ edges target).mpr (Or.inr ⟨source, edge, known⟩)

end FrKernels.Project
