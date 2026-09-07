namespace FrKernels.Git

-- fr:spec src/git.rs::line_in_range @ f00499f53d1e0b9ac75d0f75952d8b2c44538585812a0456462394a25aba1670
-- fr:signature start: usize => start: Nat; end: usize => finish: Nat; line: usize => line: Nat; return: bool => return: Bool
def lineInRange (start : Nat) (finish : Nat) (line : Nat) : Bool :=
  decide (start ≤ line ∧ line ≤ finish)

theorem line_in_range_iff (start finish line : Nat) :
    lineInRange start finish line = true ↔ start ≤ line ∧ line ≤ finish := by
  simp [lineInRange]

theorem before_range_refuses (start finish line : Nat) (h : line < start) :
    lineInRange start finish line = false := by
  simp [lineInRange]
  omega

theorem after_range_refuses (start finish line : Nat) (h : finish < line) :
    lineInRange start finish line = false := by
  simp [lineInRange]
  omega

theorem reversed_range_refuses (start finish line : Nat) (h : finish < start) :
    lineInRange start finish line = false := by
  simp [lineInRange]
  omega

theorem singleton_matches (point line : Nat) :
    lineInRange point point line = true ↔ line = point := by
  simp [lineInRange]
  omega

theorem enclosing_range_preserves_match (start finish outerStart outerFinish line : Nat)
    (left : outerStart ≤ start) (right : finish ≤ outerFinish)
    (inside : lineInRange start finish line = true) :
    lineInRange outerStart outerFinish line = true := by
  simp [lineInRange] at *
  omega

-- fr:spec src/git.rs::call_in_selection @ 2d90cfbdd483cfcbed3ee14e4e40f8ef3d794768e793d1fac571c521575c5eca
-- fr:signature incoming: bool => incoming: Bool; outgoing: bool => outgoing: Bool; include_incoming: bool => includeIncoming: Bool; include_outgoing: bool => includeOutgoing: Bool; return: bool => return: Bool
def callInSelection (incoming : Bool) (outgoing : Bool)
    (includeIncoming : Bool) (includeOutgoing : Bool) : Bool :=
  (incoming && includeIncoming) || (outgoing && includeOutgoing)

theorem no_directions_refuses (incoming outgoing : Bool) :
    callInSelection incoming outgoing false false = false := by
  cases incoming <;> cases outgoing <;> rfl

theorem no_endpoints_refuses (includeIncoming includeOutgoing : Bool) :
    callInSelection false false includeIncoming includeOutgoing = false := by
  rfl

theorem incoming_only (incoming outgoing : Bool) :
    callInSelection incoming outgoing true false = incoming := by
  cases incoming <;> cases outgoing <;> rfl

theorem outgoing_only (incoming outgoing : Bool) :
    callInSelection incoming outgoing false true = outgoing := by
  cases incoming <;> cases outgoing <;> rfl

theorem both_directions (incoming outgoing : Bool) :
    callInSelection incoming outgoing true true = (incoming || outgoing) := by
  cases incoming <;> cases outgoing <;> rfl

theorem swapping_sides_preserves_selection (incoming outgoing includeIncoming includeOutgoing : Bool) :
    callInSelection incoming outgoing includeIncoming includeOutgoing =
      callInSelection outgoing incoming includeOutgoing includeIncoming := by
  cases incoming <;> cases outgoing <;> cases includeIncoming <;> cases includeOutgoing <;> rfl

-- fr:spec src/git.rs::staging_transition_allowed @ 6efae259027fb6da965260dada7bb6817c1ffac1eb16b6527c99f2d25e4d10bc
-- fr:signature matches_before: bool => matchesBefore: Bool; matches_after: bool => matchesAfter: Bool; recovery: bool => recovery: Bool; return: bool => return: Bool
def stagingTransitionAllowed (matchesBefore : Bool) (matchesAfter : Bool) (recovery : Bool) : Bool :=
  matchesBefore || (recovery && matchesAfter)

theorem staging_requires_before (before after : Bool) :
    stagingTransitionAllowed before after false = before := by
  cases before <;> cases after <;> rfl

theorem staging_recovery_accepts_either (before after : Bool) :
    stagingTransitionAllowed before after true = (before || after) := by
  cases before <;> cases after <;> rfl

theorem staging_recovery_refuses_other :
    stagingTransitionAllowed false false true = false := rfl

abbrev StagingIndex := String → Option (Nat × String)

def replaceSelected (current target : StagingIndex) (selected : String → Bool) : StagingIndex :=
  fun path => if selected path then target path else current path

theorem staging_preserves_unselected (current target : StagingIndex) (selected : String → Bool)
    (path : String) (outside : selected path = false) :
    replaceSelected current target selected path = current path := by
  simp [replaceSelected, outside]

theorem staging_undo_restores_index (current target : StagingIndex) (selected : String → Bool) :
    replaceSelected (replaceSelected current target selected) current selected = current := by
  funext path
  simp only [replaceSelected]
  split <;> rfl

theorem staging_redo_restores_selected_result (current target : StagingIndex) (selected : String → Bool) :
    replaceSelected (replaceSelected (replaceSelected current target selected) current selected) target selected =
      replaceSelected current target selected := by
  rw [staging_undo_restores_index]

theorem staging_restore_preserves_later_unselected (original later : StagingIndex) (selected : String → Bool)
    (path : String) (outside : selected path = false) :
    replaceSelected later original selected path = later path := by
  exact staging_preserves_unselected later original selected path outside

-- fr:spec src/git.rs::commit_basis_matches @ a3140234999167c8f8c9754eda749da3e7357744a6f2efdfed12758aa1256247
-- fr:signature expected_branch: &str => expectedBranch: String; observed_branch: &str => observedBranch: String; expected_parent: &Option<String> => expectedParent: Option String; observed_parent: &Option<String> => observedParent: Option String; return: bool => return: Bool
def commitBasisMatches (expectedBranch : String) (observedBranch : String)
    (expectedParent : Option String) (observedParent : Option String) : Bool :=
  decide (expectedBranch = observedBranch ∧ expectedParent = observedParent)

theorem commit_accepts_reviewed_head (branch : String) (parent : Option String) :
    commitBasisMatches branch branch parent parent = true := by
  simp [commitBasisMatches]

theorem commit_refuses_switched_branch (expected observed : String) (parent : Option String)
    (changed : expected ≠ observed) :
    commitBasisMatches expected observed parent parent = false := by
  simp [commitBasisMatches, changed]

theorem commit_refuses_changed_parent (branch : String) (expected observed : Option String)
    (changed : expected ≠ observed) :
    commitBasisMatches branch branch expected observed = false := by
  simp [commitBasisMatches, changed]

structure CommitState where
  branch : String
  refs : String → Option String
  index : StagingIndex

def publishChecked (expectedBranch : String) (expectedParent : Option String)
    (commit : String) (state : CommitState) : Option CommitState :=
  if commitBasisMatches expectedBranch state.branch expectedParent (state.refs state.branch) then
    some { state with refs := fun name => if name = state.branch then some commit else state.refs name }
  else none

theorem commit_preserves_index (branch : String) (parent : Option String) (commit : String)
    (before after : CommitState) (published : publishChecked branch parent commit before = some after) :
    after.index = before.index := by
  unfold publishChecked at published
  split at published
  · cases published
    rfl
  · contradiction

theorem commit_preserves_other_refs (branch : String) (parent : Option String) (commit name : String)
    (before after : CommitState) (other : name ≠ before.branch)
    (published : publishChecked branch parent commit before = some after) :
    after.refs name = before.refs name := by
  unfold publishChecked at published
  split at published
  · cases published
    simp [other]
  · contradiction

-- fr:spec src/git.rs::worktree_budget_allows @ 42e4892a9187679c23705f30391bd951273e890b2c759546c3cb87671a65e934
-- fr:signature files: usize => files: Nat; bytes: usize => bytes: Nat; blob_bytes: usize => blobBytes: Nat; return: bool => return: Bool
def worktreeBudgetAllows (files : Nat) (bytes : Nat) (blobBytes : Nat) : Bool :=
  decide (files ≤ 20000 ∧ bytes ≤ 268435456 ∧ blobBytes ≤ 33554432)

theorem worktree_budget_bounds (files bytes blobBytes : Nat)
    (allowed : worktreeBudgetAllows files bytes blobBytes = true) :
    files ≤ 20000 ∧ bytes ≤ 268435456 ∧ blobBytes ≤ 33554432 := by
  simpa [worktreeBudgetAllows] using allowed

theorem worktree_budget_accepts_empty : worktreeBudgetAllows 0 0 0 = true := by decide

theorem worktree_budget_downward_closed (files bytes blobBytes fewer smaller blobSmaller : Nat)
    (allowed : worktreeBudgetAllows files bytes blobBytes = true)
    (hf : fewer ≤ files) (hb : smaller ≤ bytes) (hs : blobSmaller ≤ blobBytes) :
    worktreeBudgetAllows fewer smaller blobSmaller = true := by
  have bounds := worktree_budget_bounds files bytes blobBytes allowed
  simp only [worktreeBudgetAllows, decide_eq_true_eq]
  omega

def createFresh (before : String → Option String) (destination commit : String) : Option (String → Option String) :=
  if before destination = none then
    some (fun path => if path = destination then some commit else before path)
  else none

theorem worktree_creation_preserves_other (before after : String → Option String) (destination commit path : String)
    (created : createFresh before destination commit = some after) (other : path ≠ destination) :
    after path = before path := by
  unfold createFresh at created
  split at created
  · cases created
    simp [other]
  · contradiction

theorem worktree_creation_refuses_occupied (before : String → Option String) (destination commit : String)
    (occupied : before destination ≠ none) : createFresh before destination commit = none := by
  simp [createFresh, occupied]

end FrKernels.Git
