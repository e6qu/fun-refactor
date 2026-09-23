namespace FrKernels.Investigation

inductive State where
  | pending | ready | running | satisfied | blocked | stale
  deriving DecidableEq, Repr

def transitionAllowed (initial to : State) (prerequisites evidence : Bool) : Bool :=
  match initial, to with
  | .ready, .running => prerequisites
  | .running, .satisfied => prerequisites && evidence
  | .ready, .blocked | .running, .blocked => true
  | _, .pending => true
  | _, _ => false

theorem completion_requires_evidence (initial : State) (prerequisites : Bool) :
    transitionAllowed initial .satisfied prerequisites false = false := by
  cases initial <;> simp [transitionAllowed]

theorem completion_requires_prerequisites (initial : State) (evidence : Bool) :
    transitionAllowed initial .satisfied false evidence = false := by
  cases initial <;> simp [transitionAllowed]

theorem stale_cannot_complete (prerequisites evidence : Bool) :
    transitionAllowed .stale .satisfied prerequisites evidence = false := by
  rfl

def invalidated (inputChanged parentStale evidenceStale : Bool) : Bool :=
  inputChanged || parentStale || evidenceStale

theorem changed_input_invalidates (parentStale evidenceStale : Bool) :
    invalidated true parentStale evidenceStale = true := by
  simp [invalidated]

end FrKernels.Investigation

namespace FrKernels.Investigation

def checkScopeCovered (workspace configuration sources toolchain : Bool) : Bool :=
  workspace && configuration && sources && toolchain

theorem check_scope_requires_sources (workspace configuration toolchain : Bool) :
    checkScopeCovered workspace configuration false toolchain = false := by
  simp [checkScopeCovered]

theorem check_scope_requires_configuration (workspace sources toolchain : Bool) :
    checkScopeCovered workspace false sources toolchain = false := by
  simp [checkScopeCovered]

theorem check_scope_requires_toolchain (workspace configuration sources : Bool) :
    checkScopeCovered workspace configuration sources false = false := by
  simp [checkScopeCovered]

theorem check_scope_requires_workspace (configuration sources toolchain : Bool) :
    checkScopeCovered false configuration sources toolchain = false := by
  simp [checkScopeCovered]

end FrKernels.Investigation
