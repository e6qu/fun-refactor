namespace FrKernels.AgentDiscovery

inductive Profile where
  | compact
  | expanded
  deriving DecidableEq, Repr

def rowLimit : Profile → Nat
  | .compact => 12
  | .expanded => 30

def sourceByteLimit : Profile → Nat
  | .compact => 2048
  | .expanded => 4096

def reportByteLimit : Profile → Nat
  | .compact => 16384
  | .expanded => 32768

def budgetAdmitted (rows sourceBytes reportBytes : Nat) (profile : Profile) : Bool :=
  decide (rows ≤ rowLimit profile ∧
    sourceBytes ≤ sourceByteLimit profile ∧ reportBytes ≤ reportByteLimit profile)

-- fr:spec src/project.rs::agent_discovery_budget_admitted_code @ 889fd3e86254dde99c4347526e933d113d144aa4757c71a39be90db57c60f7e6
-- fr:signature rows: usize => rows: Nat; source_bytes: usize => sourceBytes: Nat; report_bytes: usize => reportBytes: Nat; profile: usize => profile: Nat; return: bool => return: Bool
def budgetAdmittedCode
    (rows : Nat) (sourceBytes : Nat) (reportBytes : Nat) (profile : Nat) : Bool :=
  match profile with
  | 0 => budgetAdmitted rows sourceBytes reportBytes .compact
  | 1 => budgetAdmitted rows sourceBytes reportBytes .expanded
  | _ => false

theorem admitted_respects_row_limit
    (accepted : budgetAdmitted rows sourceBytes reportBytes profile = true) :
    rows ≤ rowLimit profile := by
  have all : rows ≤ rowLimit profile ∧
      sourceBytes ≤ sourceByteLimit profile ∧ reportBytes ≤ reportByteLimit profile := by
    simpa [budgetAdmitted] using accepted
  exact all.1

theorem admitted_respects_source_limit
    (accepted : budgetAdmitted rows sourceBytes reportBytes profile = true) :
    sourceBytes ≤ sourceByteLimit profile := by
  have all : rows ≤ rowLimit profile ∧
      sourceBytes ≤ sourceByteLimit profile ∧ reportBytes ≤ reportByteLimit profile := by
    simpa [budgetAdmitted] using accepted
  exact all.2.1

theorem admitted_respects_report_limit
    (accepted : budgetAdmitted rows sourceBytes reportBytes profile = true) :
    reportBytes ≤ reportByteLimit profile := by
  have all : rows ≤ rowLimit profile ∧
      sourceBytes ≤ sourceByteLimit profile ∧ reportBytes ≤ reportByteLimit profile := by
    simpa [budgetAdmitted] using accepted
  exact all.2.2

-- fr:spec src/project.rs::agent_discovery_transition_allowed @ 7b4cc3e2712e756df4ab1704d191c928ac1ffde68b6eda2c72a7af68e1d0615b
-- fr:signature mode: usize => mode: Nat; target_supplied: bool => targetSupplied: Bool; return: bool => return: Bool
def transitionAllowed (mode : Nat) (targetSupplied : Bool) : Bool :=
  (mode == 0 && !targetSupplied) || (mode == 1 && targetSupplied)

theorem behavior_requires_target
    (accepted : transitionAllowed 1 targetSupplied = true) : targetSupplied = true := by
  simpa [transitionAllowed] using accepted

theorem names_reject_target
    (accepted : transitionAllowed 0 targetSupplied = true) : targetSupplied = false := by
  simpa [transitionAllowed] using accepted

-- fr:spec src/cache.rs::resolution_wait_action @ 78f667f2783732b631c1e7dc29b8c69817645266cbc82c4f93e8fe51bb9f94ec
-- fr:signature stale: bool => stale: Bool; expired: bool => expired: Bool; return: usize => return: Nat
def waitAction (stale : Bool) (expired : Bool) : Nat :=
  if stale then 1 else if expired then 2 else 0

theorem stale_owner_is_recovered (expired : Bool) : waitAction true expired = 1 := by
  simp [waitAction]

theorem expired_wait_does_not_continue : waitAction false true = 2 := by
  simp [waitAction]

theorem live_bounded_wait_continues : waitAction false false = 0 := by
  simp [waitAction]

-- fr:spec src/cache.rs::resolution_snapshot_publishable @ 3c392ba98203ded20cc784c0f9a316f8269ade23bf6c6d74c8d93a9c01f3b6f6
-- fr:signature owner: bool => owner: Bool; admitted: bool => admitted: Bool; return: bool => return: Bool
def snapshotPublishable (owner : Bool) (admitted : Bool) : Bool := owner && admitted

theorem publication_requires_owner
    (accepted : snapshotPublishable owner admitted = true) : owner = true := by
  have both : owner = true ∧ admitted = true := by
    simpa [snapshotPublishable] using accepted
  exact both.1

theorem publication_requires_admission
    (accepted : snapshotPublishable owner admitted = true) : admitted = true := by
  have both : owner = true ∧ admitted = true := by
    simpa [snapshotPublishable] using accepted
  exact both.2

theorem nonowner_cannot_publish (admitted : Bool) :
    snapshotPublishable false admitted = false := by
  simp [snapshotPublishable]

end FrKernels.AgentDiscovery
