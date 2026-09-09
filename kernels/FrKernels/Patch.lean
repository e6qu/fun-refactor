import Std.Tactic.BVDecide

namespace FrKernels.Patch

-- fr:spec src/history/patch.rs::git_mode @ 6a297271
-- fr:signature mode: u32 => mode: UInt32; return: u32 => return: UInt32
def gitMode (mode : UInt32) : UInt32 :=
  if mode &&& 64 == 0 then 33188 else 33261

-- fr:spec src/history/patch.rs::git_snapshot_mode @ cb1d10f4fb7e7725833299a2020b37379f2f3c063f641fe993728fab96ae7c24
-- fr:signature symlink: bool => symlink: Bool; mode: u32 => mode: UInt32; return: u32 => return: UInt32
def gitSnapshotMode (symlink : Bool) (mode : UInt32) : UInt32 :=
  if symlink then 40960 else gitMode mode

theorem symlink_mode_is_fixed (mode : UInt32) :
    gitSnapshotMode true mode = 40960 := by
  simp [gitSnapshotMode]

theorem regular_snapshot_mode_delegates (mode : UInt32) :
    gitSnapshotMode false mode = gitMode mode := by
  simp [gitSnapshotMode]

-- fr:spec src/history/patch.rs::git_mode_change_supported @ 72464470
-- fr:signature before: u32 => before: UInt32; after: u32 => after: UInt32; return: bool => return: Bool
def gitModeChangeSupported (before : UInt32) (after : UInt32) : Bool :=
  let changed := before ^^^ after
  changed &&& ~~~(73 : UInt32) == 0 && (changed == 0 || gitMode before != gitMode after)

theorem git_mode_has_two_outputs (mode : UInt32) :
    gitMode mode = 33188 ∨ gitMode mode = 33261 := by
  unfold gitMode
  split <;> simp_all

theorem symlink_mode_differs_from_regular (mode : UInt32) :
    gitSnapshotMode true mode ≠ gitSnapshotMode false mode := by
  rw [symlink_mode_is_fixed, regular_snapshot_mode_delegates]
  rcases git_mode_has_two_outputs mode with projected | projected <;> simp_all

theorem git_mode_equal_iff_owner_execute_equal (before after : UInt32) :
    gitMode before = gitMode after ↔ before &&& 64 = after &&& 64 := by
  unfold gitMode
  bv_decide

theorem git_mode_projection_is_idempotent (mode : UInt32) :
    gitMode (gitMode mode) = gitMode mode := by
  unfold gitMode
  bv_decide

theorem mode_change_supported_iff (before after : UInt32) :
    gitModeChangeSupported before after = true ↔
      before &&& ~~~(73 : UInt32) = after &&& ~~~(73 : UInt32) ∧
      (before = after ∨ before &&& 64 ≠ after &&& 64) := by
  unfold gitModeChangeSupported gitMode bne
  bv_decide

theorem unchanged_modes_are_supported (mode : UInt32) :
    gitModeChangeSupported mode mode = true := by
  rw [mode_change_supported_iff]
  simp

theorem reversing_mode_change_preserves_support (before after : UInt32) :
    gitModeChangeSupported before after = gitModeChangeSupported after before := by
  unfold gitModeChangeSupported gitMode bne
  bv_decide

theorem accepted_change_preserves_other_permission_bits (before after : UInt32)
    (accepted : gitModeChangeSupported before after = true) :
    before &&& ~~~(73 : UInt32) = after &&& ~~~(73 : UInt32) := by
  exact ((mode_change_supported_iff before after).mp accepted).1

-- fr:spec src/history/patch.rs::owner_executable_mode @ eb802953
-- fr:signature mode: u32 => mode: UInt32; executable: bool => executable: Bool; return: u32 => return: UInt32
def ownerExecutableMode (mode : UInt32) (executable : Bool) : UInt32 :=
  if executable then mode ||| 64 else mode &&& ~~~(64 : UInt32)

theorem owner_execute_setting_has_requested_bit (mode : UInt32) (executable : Bool) :
    ownerExecutableMode mode executable &&& 64 = if executable then 64 else 0 := by
  unfold ownerExecutableMode
  bv_decide

theorem owner_execute_setting_preserves_other_bits (mode : UInt32) (executable : Bool) :
    ownerExecutableMode mode executable &&& ~~~(64 : UInt32) = mode &&& ~~~(64 : UInt32) := by
  unfold ownerExecutableMode
  bv_decide

theorem owner_execute_setting_is_idempotent (mode : UInt32) (executable : Bool) :
    ownerExecutableMode (ownerExecutableMode mode executable) executable = ownerExecutableMode mode executable := by
  unfold ownerExecutableMode
  bv_decide

theorem owner_execute_setting_is_supported (mode : UInt32) (executable : Bool) :
    gitModeChangeSupported mode (ownerExecutableMode mode executable) = true := by
  unfold gitModeChangeSupported gitMode ownerExecutableMode bne
  bv_decide

theorem owner_execute_setting_has_requested_git_mode (mode : UInt32) (executable : Bool) :
    gitMode (ownerExecutableMode mode executable) = if executable then 33261 else 33188 := by
  unfold gitMode ownerExecutableMode
  bv_decide

theorem owner_execute_setting_preserves_recorded_mode_bound (mode : UInt32) (executable : Bool)
    (valid : mode ≤ 4095) : ownerExecutableMode mode executable ≤ 4095 := by
  unfold ownerExecutableMode
  bv_decide

structure FileSnapshot where
  content : String
  mode : UInt32
  symlink : Bool
  deriving DecidableEq, Repr

abbrev Snapshot := Option FileSnapshot

def patchView (snapshot : Snapshot) : Option (String × UInt32) :=
  snapshot.map (fun file => (file.content, gitSnapshotMode file.symlink file.mode))

-- fr:spec src/history/patch.rs::matches_patch_basis @ f210ed381ed6c47bd886c33b7564a17c85038df043145d88d39af3722199225d
-- fr:signature actual: &Option<Snapshot> => actual: Snapshot; expected: &Option<Snapshot> => expected: Snapshot; return: bool => return: Bool
def matchesPatchBasis (actual : Snapshot) (expected : Snapshot) : Bool :=
  decide (patchView actual = patchView expected)

theorem basis_accepts_identical_snapshot (snapshot : Snapshot) :
    matchesPatchBasis snapshot snapshot = true := by
  simp [matchesPatchBasis]

theorem full_snapshot_equality_implies_patch_basis (actual expected : Snapshot)
    (identical : actual = expected) : matchesPatchBasis actual expected = true := by
  subst actual
  exact basis_accepts_identical_snapshot expected

theorem basis_comparison_is_symmetric (actual expected : Snapshot) :
    matchesPatchBasis actual expected = matchesPatchBasis expected actual := by
  simp [matchesPatchBasis, eq_comm]

theorem basis_comparison_is_transitive (first second third : Snapshot)
    (left : matchesPatchBasis first second = true)
    (right : matchesPatchBasis second third = true) :
    matchesPatchBasis first third = true := by
  simp only [matchesPatchBasis, decide_eq_true_eq] at *
  exact left.trans right

theorem present_regular_basis_iff_content_and_owner_execute_match (actual expected : FileSnapshot)
    (actualRegular : actual.symlink = false) (expectedRegular : expected.symlink = false) :
    matchesPatchBasis (some actual) (some expected) = true ↔
      actual.content = expected.content ∧ actual.mode &&& 64 = expected.mode &&& 64 := by
  simp [matchesPatchBasis, patchView, actualRegular, expectedRegular,
    gitSnapshotMode, git_mode_equal_iff_owner_execute_equal]

theorem present_basis_preserves_kind (actual expected : FileSnapshot)
    (accepted : matchesPatchBasis (some actual) (some expected) = true) :
    actual.symlink = expected.symlink := by
  cases actualLink : actual.symlink <;> cases expectedLink : expected.symlink
  · rfl
  · simp [matchesPatchBasis, patchView, actualLink, expectedLink, gitSnapshotMode] at accepted
    rcases git_mode_has_two_outputs actual.mode with projected | projected <;> simp_all
  · simp [matchesPatchBasis, patchView, actualLink, expectedLink, gitSnapshotMode] at accepted
    rcases git_mode_has_two_outputs expected.mode with projected | projected <;> simp_all
  · rfl

theorem basis_preserves_existence (actual expected : Snapshot)
    (accepted : matchesPatchBasis actual expected = true) :
    actual.isSome = expected.isSome := by
  cases actual <;> cases expected <;> simp_all [matchesPatchBasis, patchView]

theorem patch_basis_does_not_require_full_permissions (content : String) :
    matchesPatchBasis (some ⟨content, 384, false⟩) (some ⟨content, 420, false⟩) = true ∧
      (some ⟨content, 384, false⟩ : Snapshot) ≠ some ⟨content, 420, false⟩ := by
  constructor
  · rw [present_regular_basis_iff_content_and_owner_execute_match _ _ (by rfl) (by rfl)]
    constructor
    · rfl
    · change (384 : UInt32) &&& 64 = (420 : UInt32) &&& 64
      bv_decide
  · simp [FileSnapshot.mk.injEq]

end FrKernels.Patch
