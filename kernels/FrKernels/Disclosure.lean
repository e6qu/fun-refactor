namespace FrKernels.Disclosure

def profileMaximum : Nat → Option Nat
  | 0 => some 4096
  | 1 => some 16384
  | _ => none

-- fr:spec src/project/disclose.rs::disclosure_budget_admitted @ dcc4000a3b712e47b44814e4d5d9dd37c6314ccce68e87f6e8bebc3ec0da9307
-- fr:signature serialized_bytes: usize => serializedBytes: Nat; token_limit: usize => tokenLimit: Nat; profile: usize => profile: Nat; return: bool => return: Bool
def budgetAdmitted
    (serializedBytes : Nat) (tokenLimit : Nat) (profile : Nat) : Bool :=
  match profileMaximum profile with
  | some maximum => decide (1024 ≤ tokenLimit ∧ tokenLimit ≤ maximum ∧ serializedBytes ≤ tokenLimit)
  | none => false

theorem admitted_respects_requested_limit
    (accepted : budgetAdmitted serializedBytes tokenLimit profile = true) :
    serializedBytes ≤ tokenLimit := by
  simp only [budgetAdmitted] at accepted
  split at accepted <;> simp_all

theorem compact_admission_has_fixed_ceiling
    (accepted : budgetAdmitted serializedBytes tokenLimit 0 = true) : tokenLimit ≤ 4096 := by
  have bounds : 1024 ≤ tokenLimit ∧ tokenLimit ≤ 4096 ∧ serializedBytes ≤ tokenLimit := by
    simpa [budgetAdmitted, profileMaximum] using accepted
  exact bounds.2.1

theorem expanded_admission_has_fixed_ceiling
    (accepted : budgetAdmitted serializedBytes tokenLimit 1 = true) : tokenLimit ≤ 16384 := by
  have bounds : 1024 ≤ tokenLimit ∧ tokenLimit ≤ 16384 ∧ serializedBytes ≤ tokenLimit := by
    simpa [budgetAdmitted, profileMaximum] using accepted
  exact bounds.2.1

theorem unknown_profile_is_refused (profile : Nat) (invalid : 2 ≤ profile) :
    budgetAdmitted serializedBytes tokenLimit profile = false := by
  rcases profile with _ | profile <;> simp_all [budgetAdmitted, profileMaximum]
  rcases profile with _ | profile <;> simp_all

-- fr:spec src/project/disclose.rs::disclosure_transition_allowed @ cbdd48221ecb7558e34777cc4034d160a2e716edd63b4fdf7c44081a98759bdf
-- fr:signature kind: usize => kind: Nat; offset: usize => offset: Nat; total: usize => total: Nat; return: bool => return: Bool
def transitionAllowed (kind : Nat) (offset : Nat) (total : Nat) : Bool :=
  match kind with
  | 0 => decide (offset ≤ total)
  | 1 => decide (offset ≤ total)
  | _ => false

theorem admitted_transition_stays_within_committed_extent
    (accepted : transitionAllowed kind offset total = true) : offset ≤ total := by
  rcases kind with _ | kind <;> simp_all [transitionAllowed]
  rcases kind with _ | kind <;> simp_all

theorem unknown_transition_is_refused (kind : Nat) (invalid : 2 ≤ kind) :
    transitionAllowed kind offset total = false := by
  rcases kind with _ | kind <;> simp_all [transitionAllowed]
  rcases kind with _ | kind <;> simp_all

-- fr:spec src/project/disclose.rs::disclosure_frontier_after @ 715f5151dab4e57fab38b9eecf1ac8319ccea725167b699dda8446dacfd7907c
-- fr:signature hidden: u64 => hidden: Nat; children: u64 => children: Nat; return: Option<u64> => return: Option Nat
def frontierAfter (hidden : Nat) (children : Nat) : Option Nat :=
  if hidden = 0 then none
  else if hidden - 1 + children ≤ 18446744073709551615
  then some (hidden - 1 + children)
  else none

theorem reveal_replaces_exactly_one_hole (present : 0 < hidden)
    (bounded : hidden - 1 + children ≤ 18446744073709551615) :
    frontierAfter hidden children = some (hidden - 1 + children) := by
  simp [frontierAfter, Nat.ne_of_gt present, bounded]

theorem empty_frontier_cannot_be_revealed : frontierAfter 0 children = none := by
  simp [frontierAfter]

end FrKernels.Disclosure
