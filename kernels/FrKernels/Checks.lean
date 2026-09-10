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

-- fr:spec src/checks.rs::check_requirement_satisfied @ 297b24d458e376b0c70238d024c5fcfb869670afbab0ccbabcfaf05bf16e42bc
-- fr:signature requirement_present: bool => requirementPresent: Bool; configuration_matches: bool => configurationMatches: Bool; check_names_match: bool => checkNamesMatch: Bool; return: bool => return: Bool
def checkRequirementSatisfied
    (requirementPresent : Bool) (configurationMatches : Bool) (checkNamesMatch : Bool) : Bool :=
  !requirementPresent || configurationMatches && checkNamesMatch

theorem check_requirement_satisfied_iff_absent_or_exact
    (requirementPresent configurationMatches checkNamesMatch : Bool) :
    checkRequirementSatisfied requirementPresent configurationMatches checkNamesMatch = true ↔
      requirementPresent = false ∨ configurationMatches = true ∧ checkNamesMatch = true := by
  cases requirementPresent <;> cases configurationMatches <;> cases checkNamesMatch <;> decide

theorem absent_check_requirement_accepts_any_evidence
    (configurationMatches checkNamesMatch : Bool) :
    checkRequirementSatisfied false configurationMatches checkNamesMatch = true := by
  cases configurationMatches <;> cases checkNamesMatch <;> decide

theorem present_check_requirement_rejects_different_names
    (configurationMatches : Bool) :
    checkRequirementSatisfied true configurationMatches false = false := by
  cases configurationMatches <;> decide

end FrKernels.Checks
