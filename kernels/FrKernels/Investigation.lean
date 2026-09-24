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

def compilerEvidenceComplete (execution protocol fromStart noRemaining : Bool) : Bool :=
  execution && protocol && fromStart && noRemaining

theorem compiler_requires_execution (protocol fromStart noRemaining : Bool) :
    compilerEvidenceComplete false protocol fromStart noRemaining = false := by
  simp [compilerEvidenceComplete]

theorem compiler_requires_protocol (execution fromStart noRemaining : Bool) :
    compilerEvidenceComplete execution false fromStart noRemaining = false := by
  simp [compilerEvidenceComplete]

theorem compiler_requires_prefix (execution protocol noRemaining : Bool) :
    compilerEvidenceComplete execution protocol false noRemaining = false := by
  simp [compilerEvidenceComplete]

theorem compiler_requires_suffix (execution protocol fromStart : Bool) :
    compilerEvidenceComplete execution protocol fromStart false = false := by
  simp [compilerEvidenceComplete]

theorem compiler_all_inputs_complete : compilerEvidenceComplete true true true true = true := by
  rfl

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
