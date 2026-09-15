namespace FrKernels.AgentSession

-- fr:spec src/project/task_change.rs::agent_session_step @ 6486fe489f1c1f5a49bea355ae7fc15a338cae0d858f907caaf4fcc083404a9e
-- fr:signature state: usize => state: Nat; action: usize => action: Nat; preview_valid: bool => previewValid: Bool; manifest_matches: bool => manifestMatches: Bool; basis_matches: bool => basisMatches: Bool; return: usize => return: Nat
def step
    (state : Nat)
    (action : Nat)
    (previewValid : Bool)
    (manifestMatches : Bool)
    (basisMatches : Bool) : Nat :=
  match state, action, previewValid, manifestMatches, basisMatches with
  | 0, 0, true, _, _ => 1
  | 1, 1, true, true, true => 2
  | _, _, _, _, _ => 3

theorem execute_requires_review_and_exact_inputs
    (state action : Nat)
    (previewValid manifestMatches basisMatches : Bool)
    (accepted : step state action previewValid manifestMatches basisMatches = 2) :
    state = 1 ∧ action = 1 ∧ previewValid = true ∧
      manifestMatches = true ∧ basisMatches = true := by
  cases state with
  | zero =>
      cases action <;> cases previewValid <;> cases manifestMatches <;>
        cases basisMatches <;> simp_all [step]
  | succ state =>
      cases state with
      | zero =>
          cases action with
          | zero =>
              cases previewValid <;> cases manifestMatches <;>
                cases basisMatches <;> simp_all [step]
          | succ action =>
              cases action <;> cases previewValid <;> cases manifestMatches <;>
                cases basisMatches <;> simp_all [step]
      | succ state =>
          cases action <;> cases previewValid <;> cases manifestMatches <;>
            cases basisMatches <;> simp_all [step]

theorem draft_cannot_execute
    (previewValid manifestMatches basisMatches : Bool) :
    step 0 1 previewValid manifestMatches basisMatches = 3 := by
  cases previewValid <;> cases manifestMatches <;> cases basisMatches <;> rfl

theorem review_cannot_skip_preview
    (manifestMatches basisMatches : Bool) :
    step 0 0 false manifestMatches basisMatches = 3 := by
  cases manifestMatches <;> cases basisMatches <;> rfl

end FrKernels.AgentSession
