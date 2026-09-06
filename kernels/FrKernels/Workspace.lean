import FrKernels.Project

namespace FrKernels.Project

private theorem unique_insert (node : Nat) (nodes : List Nat) (unique : nodes.Nodup) :
    (nodes.insert node).Nodup := by
  unfold List.insert
  split
  · exact unique
  · simp_all

private theorem canonical_unique (nodes : List Nat) : (canonicalMembers nodes).Nodup := by
  apply (List.mergeSort_perm _ _).symm.nodup
  induction nodes with
  | nil => simp
  | cons head tail ih => exact unique_insert head _ ih

private theorem canonical_ordered (nodes : List Nat) :
    (canonicalMembers nodes).Pairwise (· ≤ ·) := by
  have h := List.pairwise_mergeSort (le := fun a b : Nat => decide (a ≤ b))
    (fun _ _ _ ab bc => by simp_all; omega)
    (fun a b => by simp only [Bool.or_eq_true, decide_eq_true_eq]; exact Nat.le_total a b)
    (nodes.foldr List.insert [])
  simpa only [canonicalMembers, decide_eq_true_eq] using h

private theorem ordered_unique_ext (left right : List Nat)
    (lo : left.Pairwise (· ≤ ·)) (ro : right.Pairwise (· ≤ ·))
    (lu : left.Nodup) (ru : right.Nodup)
    (same : ∀ node, node ∈ left ↔ node ∈ right) : left = right := by
  induction left generalizing right with
  | nil =>
    symm
    exact List.eq_nil_iff_forall_not_mem.mpr (fun n h => List.not_mem_nil ((same n).mpr h))
  | cons head tail ih =>
    cases right with
    | nil => have := (same head).mp (by simp); simp at this
    | cons other rest =>
      have ho : head ≤ other := by
        rcases List.mem_cons.mp ((same other).mpr (by simp)) with eq | member
        · omega
        · exact (List.pairwise_cons.mp lo).1 other member
      have oh : other ≤ head := by
        rcases List.mem_cons.mp ((same head).mp (by simp)) with eq | member
        · omega
        · exact (List.pairwise_cons.mp ro).1 head member
      have eq : head = other := Nat.le_antisymm ho oh
      subst other
      congr 1
      apply ih rest lo.tail ro.tail lu.tail ru.tail
      intro node
      have := same node
      have hn := (List.nodup_cons.mp lu).1
      have rn := (List.nodup_cons.mp ru).1
      simp only [List.mem_cons] at this
      by_cases eq : node = head
      · subst node; simp [hn, rn]
      · simpa [eq] using this

private theorem canonical_ext (left right : List Nat)
    (same : ∀ node, node ∈ left ↔ node ∈ right) : canonicalMembers left = canonicalMembers right :=
  ordered_unique_ext _ _ (canonical_ordered left) (canonical_ordered right)
    (canonical_unique left) (canonical_unique right)
    (fun node => by simpa only [canonical_membership] using same node)

private def membershipUniverse (seeds : List Nat) (edges : List (Nat × Nat)) : List Nat :=
  seeds ++ edges.map Prod.snd

private theorem rounds_in_universe (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat) :
    ∀ node ∈ membershipRounds seeds edges rounds, node ∈ membershipUniverse seeds edges := by
  apply membership_rounds_stay_in_closed_superset
  · intro node known
    simp [membershipUniverse, known]
  · intro source target _ edge
    simp only [membershipUniverse, List.mem_append, List.mem_map]
    exact Or.inr ⟨(source, target), edge, rfl⟩

private def missingMembers (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat) : List Nat :=
  (membershipUniverse seeds edges).filter (fun node => node ∉ membershipRounds seeds edges rounds)

private theorem rounds_ext (seeds : List Nat) (edges : List (Nat × Nat)) (a b : Nat)
    (same : ∀ node, node ∈ membershipRounds seeds edges a ↔ node ∈ membershipRounds seeds edges b) :
    membershipRounds seeds edges a = membershipRounds seeds edges b := by
  cases a <;> cases b <;>
    apply canonical_ext <;> simpa only [membershipRounds, workspaceMembershipStep, canonical_membership] using same

private theorem missing_decreases (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat) :
    List.Sublist (missingMembers seeds edges (rounds + 1)) (missingMembers seeds edges rounds) := by
  have filtered : missingMembers seeds edges (rounds + 1) =
      (missingMembers seeds edges rounds).filter (fun node => node ∉ membershipRounds seeds edges (rounds + 1)) := by
    simp only [missingMembers, List.filter_filter]
    apply List.filter_congr
    intro node _
    have preserves := membership_step_preserves (membershipRounds seeds edges rounds) edges node
    by_cases old : node ∈ membershipRounds seeds edges rounds
    · have new := preserves old
      simp [old, membershipRounds, new]
    · simp [old]
  rw [filtered]
  exact List.filter_sublist

private theorem equal_missing_stabilizes (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat)
    (same : missingMembers seeds edges (rounds + 1) = missingMembers seeds edges rounds) :
    membershipRounds seeds edges (rounds + 1) = membershipRounds seeds edges rounds := by
  apply rounds_ext
  intro node
  constructor
  · intro present
    by_cases absent : node ∈ membershipRounds seeds edges rounds
    · exact absent
    have missing : node ∈ missingMembers seeds edges rounds := by
      simp only [missingMembers, List.mem_filter, decide_eq_true_eq]
      exact ⟨rounds_in_universe seeds edges (rounds + 1) node present, absent⟩
    rw [← same] at missing
    simp [missingMembers, present] at missing
  · exact membership_step_preserves _ edges node

private theorem nonstable_decreases (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat)
    (changing : membershipRounds seeds edges (rounds + 1) ≠ membershipRounds seeds edges rounds) :
    (missingMembers seeds edges (rounds + 1)).length < (missingMembers seeds edges rounds).length := by
  have sub := missing_decreases seeds edges rounds
  have unequal : missingMembers seeds edges (rounds + 1) ≠ missingMembers seeds edges rounds :=
    fun same => changing (equal_missing_stabilizes seeds edges rounds same)
  have le := sub.length_le
  have ne : (missingMembers seeds edges (rounds + 1)).length ≠ (missingMembers seeds edges rounds).length :=
    fun same => unequal (sub.eq_of_length same)
  omega

private theorem initial_missing_bound (seeds : List Nat) (edges : List (Nat × Nat)) :
    (missingMembers seeds edges 0).length ≤ edges.length := by
  have empty : seeds.filter (fun node => node ∉ canonicalMembers seeds) = [] := by
    apply List.filter_eq_nil_iff.mpr
    intro node known
    simp [canonical_membership, known]
  simp only [missingMembers, membershipUniverse, membershipRounds, List.filter_append, empty, List.nil_append]
  simpa using List.length_filter_le (fun node => node ∉ canonicalMembers seeds) (edges.map Prod.snd)

private theorem stable_next (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat)
    (stable : membershipRounds seeds edges (rounds + 1) = membershipRounds seeds edges rounds) :
    membershipRounds seeds edges (rounds + 2) = membershipRounds seeds edges (rounds + 1) :=
  congrArg (fun members => workspaceMembershipStep members edges) stable

private theorem stable_or_progress (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat) :
    membershipRounds seeds edges (rounds + 1) = membershipRounds seeds edges rounds ∨
      rounds + (missingMembers seeds edges rounds).length ≤ edges.length := by
  induction rounds with
  | zero => right; simpa using initial_missing_bound seeds edges
  | succ rounds ih =>
    by_cases stable : membershipRounds seeds edges (rounds + 1) = membershipRounds seeds edges rounds
    · exact Or.inl (stable_next seeds edges rounds stable)
    · have bound := ih.resolve_left stable
      have decrease := nonstable_decreases seeds edges rounds stable
      exact Or.inr (by omega)

theorem membership_converges (seeds : List Nat) (edges : List (Nat × Nat)) :
    workspaceMembershipStep (membershipRounds seeds edges edges.length) edges =
      membershipRounds seeds edges edges.length := by
  rcases stable_or_progress seeds edges edges.length with stable | bound
  · exact stable
  · have sub := missing_decreases seeds edges edges.length
    apply equal_missing_stabilizes
    apply sub.eq_of_length_le
    omega

def workspaceClosure (seeds : List Nat) (edges : List (Nat × Nat)) : List Nat :=
  membershipRounds seeds edges edges.length

theorem workspace_closure_exact (seeds : List Nat) (edges : List (Nat × Nat)) (node : Nat) :
    node ∈ workspaceClosure seeds edges ↔ MemberReachable seeds edges node :=
  stabilized_membership_is_exact seeds edges edges.length (membership_converges seeds edges) node

theorem workspace_closure_fixed (seeds : List Nat) (edges : List (Nat × Nat)) :
    workspaceMembershipStep (workspaceClosure seeds edges) edges = workspaceClosure seeds edges :=
  membership_converges seeds edges

theorem membership_stays_converged (seeds : List Nat) (edges : List (Nat × Nat)) (extra : Nat) :
    membershipRounds seeds edges (edges.length + extra) = workspaceClosure seeds edges := by
  induction extra with
  | zero => rfl
  | succ extra ih =>
    change workspaceMembershipStep (membershipRounds seeds edges (edges.length + extra)) edges = _
    rw [ih]
    exact workspace_closure_fixed seeds edges

theorem workspace_closure_least (seeds : List Nat) (edges : List (Nat × Nat))
    (allowed : Nat → Prop) (includes : ∀ node ∈ seeds, allowed node)
    (closed : ∀ source target, allowed source → (source, target) ∈ edges → allowed target)
    (node : Nat) (present : node ∈ workspaceClosure seeds edges) : allowed node :=
  reachable_in_closed_superset seeds edges allowed includes closed node
    ((workspace_closure_exact seeds edges node).mp present)

end FrKernels.Project
