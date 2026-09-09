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

-- fr:spec src/git.rs::staging_record_compactable @ b950d8b98a91ce3745e26cc2ec39494ed41f90379043541e41f344fe52dbbdda
-- fr:signature detailed: bool => detailed: Bool; pending: bool => pending: Bool; retained: bool => retained: Bool; return: bool => return: Bool
def stagingRecordCompactable (detailed : Bool) (pending : Bool) (retained : Bool) : Bool :=
  detailed && !pending && !retained

theorem staging_compaction_requires_detail (detailed pending retained : Bool)
    (allowed : stagingRecordCompactable detailed pending retained = true) : detailed = true := by
  cases detailed <;> cases pending <;> cases retained <;>
    simp_all [stagingRecordCompactable]

theorem staging_compaction_refuses_pending (detailed retained : Bool) :
    stagingRecordCompactable detailed true retained = false := by
  cases detailed <;> cases retained <;> rfl

theorem staging_compaction_preserves_retained (detailed pending : Bool) :
    stagingRecordCompactable detailed pending true = false := by
  cases detailed <;> cases pending <;> rfl

def compactStagingPayload (payload : Option String) (selected : Bool) : Option String :=
  if selected then none else payload

theorem staging_compaction_preserves_unselected (payload : Option String) :
    compactStagingPayload payload false = payload := by rfl

theorem staging_compaction_discards_selected (payload : Option String) :
    compactStagingPayload payload true = none := by rfl

theorem staging_compaction_is_idempotent (payload : Option String) (selected : Bool) :
    compactStagingPayload (compactStagingPayload payload selected) selected =
      compactStagingPayload payload selected := by
  cases selected <;> rfl

-- fr:spec src/git.rs::staging_crash_state_requires_review @ 9440dae3c9d4a485fc6d132214d00886e44bc6afc9aa10f2d25e14d83ef0c7a2
-- fr:signature pending: bool => pending: Bool; index_lock: bool => indexLock: Bool; preparation: bool => preparation: Bool; return: bool => return: Bool
def stagingCrashStateRequiresReview (pending : Bool) (indexLock : Bool)
    (preparation : Bool) : Bool :=
  pending || indexLock || preparation

theorem staging_clean_requires_no_crash_evidence (pending indexLock preparation : Bool) :
    stagingCrashStateRequiresReview pending indexLock preparation = false ↔
      pending = false ∧ indexLock = false ∧ preparation = false := by
  cases pending <;> cases indexLock <;> cases preparation <;>
    simp [stagingCrashStateRequiresReview]

theorem staging_pending_requires_review (indexLock preparation : Bool) :
    stagingCrashStateRequiresReview true indexLock preparation = true := by rfl

theorem staging_lock_requires_review (pending preparation : Bool) :
    stagingCrashStateRequiresReview pending true preparation = true := by
  cases pending <;> rfl

theorem staging_preparation_requires_review (pending indexLock : Bool) :
    stagingCrashStateRequiresReview pending indexLock true = true := by
  cases pending <;> cases indexLock <;> rfl

-- fr:spec src/git.rs::staging_index_entry_replayable @ df092a5b08c85adcc69352a4bab8df5a80bd256925d5dfae78235d38e0e8abef
-- fr:signature stage_zero: bool => stageZero: Bool; intent_to_add: bool => intentToAdd: Bool; _assume_unchanged: bool => assumeUnchanged: Bool; _skip_worktree: bool => skipWorktree: Bool; return: bool => return: Bool
def stagingIndexEntryReplayable (stageZero : Bool) (intentToAdd : Bool)
    (assumeUnchanged : Bool) (skipWorktree : Bool) : Bool :=
  stageZero && !intentToAdd &&
    (assumeUnchanged || !assumeUnchanged) && (skipWorktree || !skipWorktree)

theorem staging_replay_requires_stage_zero (intent assume skip : Bool) :
    stagingIndexEntryReplayable false intent assume skip = false := by rfl

theorem staging_replay_refuses_intent_to_add (stageZero assume skip : Bool) :
    stagingIndexEntryReplayable stageZero true assume skip = false := by
  cases stageZero <;> rfl

theorem staging_replay_accepts_ordinary_flags (assume skip : Bool) :
    stagingIndexEntryReplayable true false assume skip = true := by
  cases assume <;> cases skip <;> rfl

theorem staging_replay_is_independent_of_ordinary_flags
    (stageZero intent assumeLeft skipLeft assumeRight skipRight : Bool) :
    stagingIndexEntryReplayable stageZero intent assumeLeft skipLeft =
      stagingIndexEntryReplayable stageZero intent assumeRight skipRight := by
  cases stageZero <;> cases intent <;> cases assumeLeft <;> cases skipLeft <;>
    cases assumeRight <;> cases skipRight <;> rfl

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

-- fr:spec src/git.rs::worktree_recovery_file_allowed @ 6e5dee8d0967f3ebc7e3bacfae326fd311c58af820a88f36af26b58b642a1733
-- fr:signature present: bool => present: Bool; bytes_match: bool => bytesMatch: Bool; mode_matches: bool => modeMatches: Bool; return: bool => return: Bool
def worktreeRecoveryFileAllowed (present : Bool) (bytesMatch : Bool) (modeMatches : Bool) : Bool :=
  !present || (bytesMatch && modeMatches)

theorem recovery_accepts_missing (bytesMatch modeMatches : Bool) :
    worktreeRecoveryFileAllowed false bytesMatch modeMatches = true := by
  cases bytesMatch <;> cases modeMatches <;> rfl

theorem recovery_requires_existing_match (bytesMatch modeMatches : Bool) :
    worktreeRecoveryFileAllowed true bytesMatch modeMatches = (bytesMatch && modeMatches) := by
  cases bytesMatch <;> cases modeMatches <;> rfl

-- fr:spec src/git.rs::worktree_configuration_allowed @ 1bc754961f2104c4847a9da4d54ff0100ab8d7d33bce0e4760cb2be0d518b0bf
-- fr:signature reviewed_mode: bool => reviewedMode: Bool; observed_mode: bool => observedMode: Bool; config_present: bool => configPresent: Bool; config_regular: bool => configRegular: Bool; return: bool => return: Bool
def worktreeConfigurationAllowed (reviewedMode : Bool) (observedMode : Bool)
    (configPresent : Bool) (configRegular : Bool) : Bool :=
  decide (reviewedMode = observedMode) && (!configPresent || configRegular)

theorem worktree_configuration_requires_reviewed_mode
    (reviewed observed present regular : Bool)
    (allowed : worktreeConfigurationAllowed reviewed observed present regular = true) :
    reviewed = observed := by
  cases reviewed <;> cases observed <;> cases present <;> cases regular <;>
    simp_all [worktreeConfigurationAllowed]

theorem worktree_configuration_requires_regular_file
    (reviewed observed regular : Bool) :
    worktreeConfigurationAllowed reviewed observed true regular =
      (decide (reviewed = observed) && regular) := by
  cases reviewed <;> cases observed <;> cases regular <;> rfl

theorem worktree_configuration_accepts_absent_file (mode : Bool) :
    worktreeConfigurationAllowed mode mode false false = true := by
  cases mode <;> rfl

-- fr:spec src/git.rs::worktree_prepared_recovery_allowed @ 2ec191ee1b6f1747fa4c2f30f188d3a82e052d55437c3b2da03efa6786344eb2
-- fr:signature receipt_present: bool => receiptPresent: Bool; preparation_matches: bool => preparationMatches: Bool; registration_matches: bool => registrationMatches: Bool; return: bool => return: Bool
def worktreePreparedRecoveryAllowed (receiptPresent : Bool) (preparationMatches : Bool)
    (registrationMatches : Bool) : Bool :=
  !receiptPresent && preparationMatches && registrationMatches

theorem prepared_recovery_requires_no_receipt (preparation registration : Bool) :
    worktreePreparedRecoveryAllowed true preparation registration = false := by rfl

theorem prepared_recovery_requires_matching_preparation (receipt registration : Bool) :
    worktreePreparedRecoveryAllowed receipt false registration = false := by
  cases receipt <;> rfl

theorem prepared_recovery_requires_matching_registration (receipt preparation : Bool) :
    worktreePreparedRecoveryAllowed receipt preparation false = false := by
  cases receipt <;> cases preparation <;> rfl

theorem prepared_recovery_accepts_complete_evidence :
    worktreePreparedRecoveryAllowed false true true = true := by rfl

def resumeFile (before : Option (String × Nat)) (target : String × Nat) : Option (String × Nat) :=
  if before = none ∨ before = some target then some target else none

theorem recovery_preserves_existing_file (before target after : String × Nat)
    (accepted : resumeFile (some before) target = some after) : after = before := by
  simp only [resumeFile, Option.some_ne_none, false_or, Option.some.injEq] at accepted
  split at accepted
  · rename_i same
    simpa [same] using accepted.symm
  · contradiction

theorem recovery_fills_missing_file (target : String × Nat) : resumeFile none target = some target := by
  simp [resumeFile]

theorem recovery_refuses_changed_file (before target : String × Nat) (changed : before ≠ target) :
    resumeFile (some before) target = none := by
  simp [resumeFile, changed]

-- fr:spec src/git.rs::worktree_removal_file_allowed @ 9f88589cab69e74e85d6194c51d37505cf95f76958b3f72822c9c9d845e5dad1
-- fr:signature identity_matches: bool => identityMatches: Bool; bytes_match: bool => bytesMatch: Bool; mode_matches: bool => modeMatches: Bool; return: bool => return: Bool
def worktreeRemovalFileAllowed (identityMatches : Bool) (bytesMatch : Bool) (modeMatches : Bool) : Bool :=
  identityMatches && bytesMatch && modeMatches

theorem removal_requires_all_matches (identityMatches bytesMatch modeMatches : Bool)
    (allowed : worktreeRemovalFileAllowed identityMatches bytesMatch modeMatches = true) :
    identityMatches = true ∧ bytesMatch = true ∧ modeMatches = true := by
  cases identityMatches <;> cases bytesMatch <;> cases modeMatches <;> simp_all [worktreeRemovalFileAllowed]

def removeSelected (before : String → Option String) (selected : List String) : String → Option String :=
  fun path => if path ∈ selected then none else before path

theorem removal_preserves_unselected (before : String → Option String) (selected : List String) (path : String)
    (other : path ∉ selected) : removeSelected before selected path = before path := by
  simp [removeSelected, other]

theorem removal_erases_selected (before : String → Option String) (selected : List String) (path : String)
    (chosen : path ∈ selected) : removeSelected before selected path = none := by
  simp [removeSelected, chosen]

-- fr:spec src/git.rs::worktree_removal_resume_allowed @ 043123a18970992a594713f5248af27824291da933321641a3658f40fb199f1c
-- fr:signature present: bool => present: Bool; identity_matches: bool => identityMatches: Bool; bytes_match: bool => bytesMatch: Bool; mode_matches: bool => modeMatches: Bool; return: bool => return: Bool
def worktreeRemovalResumeAllowed (present : Bool) (identityMatches : Bool) (bytesMatch : Bool) (modeMatches : Bool) : Bool :=
  !present || worktreeRemovalFileAllowed identityMatches bytesMatch modeMatches

theorem removal_resume_accepts_absent (identityMatches bytesMatch modeMatches : Bool) :
    worktreeRemovalResumeAllowed false identityMatches bytesMatch modeMatches = true := by
  rfl

theorem removal_resume_requires_existing_match (identityMatches bytesMatch modeMatches : Bool)
    (allowed : worktreeRemovalResumeAllowed true identityMatches bytesMatch modeMatches = true) :
    identityMatches = true ∧ bytesMatch = true ∧ modeMatches = true := by
  exact removal_requires_all_matches identityMatches bytesMatch modeMatches allowed

theorem selected_removal_is_idempotent (before : String → Option String) (selected : List String) :
    removeSelected (removeSelected before selected) selected = removeSelected before selected := by
  funext path
  by_cases chosen : path ∈ selected <;> simp [removeSelected, chosen]

-- fr:spec src/git.rs::worktree_branch_selection_allowed @ a9856aa6dde12b7063b31ee2c2603935a5f50e3ceec13a26c27cbb5712cf8995
-- fr:signature existing: bool => existing: Bool; present: bool => present: Bool; occupied: bool => occupied: Bool; return: bool => return: Bool
def worktreeBranchSelectionAllowed (existing : Bool) (present : Bool) (occupied : Bool) : Bool :=
  !occupied && (existing == present)

theorem worktree_branch_selection_requires_unused (existing present occupied : Bool)
    (allowed : worktreeBranchSelectionAllowed existing present occupied = true) : occupied = false := by
  cases existing <;> cases present <;> cases occupied <;> simp_all [worktreeBranchSelectionAllowed]

theorem worktree_branch_selection_matches_presence (existing present occupied : Bool)
    (allowed : worktreeBranchSelectionAllowed existing present occupied = true) : existing = present := by
  cases existing <;> cases present <;> cases occupied <;> simp_all [worktreeBranchSelectionAllowed]

def attachExisting (refs : String → Option String) (branch commit : String) : Option (String → Option String) :=
  if refs branch = some commit then some refs else none

theorem attachment_preserves_refs (refs after : String → Option String) (branch commit : String)
    (attached : attachExisting refs branch commit = some after) : after = refs := by
  unfold attachExisting at attached
  split at attached
  · cases attached
    rfl
  · contradiction

-- fr:spec src/git.rs::worktree_archive_compaction_allowed @ 60422154d4888d098c8a17d82b7d29cbc4da8b5403033342bcc10dd269569ecd
-- fr:signature complete: bool => complete: Bool; checkout_absent: bool => checkoutAbsent: Bool; metadata_absent: bool => metadataAbsent: Bool; unlocked: bool => unlocked: Bool; return: bool => return: Bool
def worktreeArchiveCompactionAllowed (complete : Bool) (checkoutAbsent : Bool) (metadataAbsent : Bool) (unlocked : Bool) : Bool :=
  complete && checkoutAbsent && metadataAbsent && unlocked

theorem archive_compaction_requires_completion_and_absence (complete checkoutAbsent metadataAbsent unlocked : Bool)
    (allowed : worktreeArchiveCompactionAllowed complete checkoutAbsent metadataAbsent unlocked = true) :
    complete = true ∧ checkoutAbsent = true ∧ metadataAbsent = true ∧ unlocked = true := by
  cases complete <;> cases checkoutAbsent <;> cases metadataAbsent <;> cases unlocked <;>
    simp_all [worktreeArchiveCompactionAllowed]

def compactArchive (archive : String × Option String) : String × Option String := (archive.1, none)

theorem archive_compaction_preserves_audit (archive : String × Option String) :
    (compactArchive archive).1 = archive.1 := by rfl

theorem archive_compaction_is_idempotent (archive : String × Option String) :
    compactArchive (compactArchive archive) = compactArchive archive := by rfl

end FrKernels.Git
