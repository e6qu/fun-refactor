namespace FrKernels.Checks

-- fr:spec src/checks.rs::check_evidence_acceptable @ 6da8491503f681a179efc1f96e2f49d23d94ddc0431c94f2dab7b7c992f55fb4
-- fr:signature executed: bool => executed: Bool; commands_passed: bool => commandsPassed: Bool; configuration_stable: bool => configurationStable: Bool; source_snapshot_stable: bool => sourceSnapshotStable: Bool; return: bool => return: Bool
def checkEvidenceAcceptable
    (executed : Bool) (commandsPassed : Bool) (configurationStable : Bool)
    (sourceSnapshotStable : Bool) : Bool :=
  executed && commandsPassed && configurationStable && sourceSnapshotStable

theorem check_evidence_acceptable_iff
    (executed commandsPassed configurationStable sourceSnapshotStable : Bool) :
    checkEvidenceAcceptable executed commandsPassed configurationStable sourceSnapshotStable = true ↔
      executed = true ∧ commandsPassed = true ∧ configurationStable = true ∧
        sourceSnapshotStable = true := by
  cases executed <;> cases commandsPassed <;> cases configurationStable <;>
    cases sourceSnapshotStable <;> decide

theorem failed_command_rejects_check_evidence
    (executed configurationStable sourceSnapshotStable : Bool) :
    checkEvidenceAcceptable executed false configurationStable sourceSnapshotStable = false := by
  cases executed <;> cases configurationStable <;> cases sourceSnapshotStable <;> decide

theorem configuration_drift_rejects_check_evidence
    (executed commandsPassed sourceSnapshotStable : Bool) :
    checkEvidenceAcceptable executed commandsPassed false sourceSnapshotStable = false := by
  cases executed <;> cases commandsPassed <;> cases sourceSnapshotStable <;> decide

theorem source_drift_rejects_check_evidence
    (executed commandsPassed configurationStable : Bool) :
    checkEvidenceAcceptable executed commandsPassed configurationStable false = false := by
  cases executed <;> cases commandsPassed <;> cases configurationStable <;> decide

end FrKernels.Checks
