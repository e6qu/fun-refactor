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
