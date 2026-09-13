namespace FrKernels.ProjectIdentity

/-- The ordered, versioned inputs whose serialization is hashed into a project revision. -/
structure RevisionMaterial where
  schema : String
  selected : String
  packageVersion : String
  factSemantics : String
  scanPolicy : List Nat
  files : List (String × String × String × List String)
  skipped : List (String × String)
  manifests : List String
  deriving BEq, Repr

/-- Hashing is abstract here. The theorem states the exact collision-resistance assumption. -/
def projectRevision (digest : RevisionMaterial → String) (material : RevisionMaterial) : String :=
  digest material

theorem same_material_same_revision (digest : RevisionMaterial → String)
    (left right : RevisionMaterial) (same : left = right) :
    projectRevision digest left = projectRevision digest right := by
  subst right
  rfl

theorem revision_equality_reflects_material_equality (digest : RevisionMaterial → String)
    (collisionFree : Function.Injective digest) (left right : RevisionMaterial) :
    projectRevision digest left = projectRevision digest right ↔ left = right := by
  constructor
  · intro sameRevision
    exact collisionFree sameRevision
  · intro same
    subst right
    rfl

theorem changed_material_changes_revision (digest : RevisionMaterial → String)
    (collisionFree : Function.Injective digest) (left right : RevisionMaterial)
    (changed : left ≠ right) : projectRevision digest left ≠ projectRevision digest right := by
  intro sameRevision
  exact changed (collisionFree sameRevision)

structure ResolutionEntry where
  target : Option Nat
  confidence : Nat
  deriving BEq, Repr

-- fr:spec src/index.rs::resolution_snapshot_admitted @ 2b5cce8b0e45617d17bc0f6f56eb3b3dd3aa25804515fc9b997b2bab77374dae
-- fr:signature reference_count: usize => referenceCount: Nat; symbol_count: usize => symbolCount: Nat; entries: &[(Option<SymbolId>, Confidence)] => entries: List ResolutionEntry; return: bool => return: Bool
def resolutionSnapshotAdmitted
    (referenceCount : Nat) (symbolCount : Nat) (entries : List ResolutionEntry) : Bool :=
  decide (entries.length = referenceCount) && entries.all fun entry =>
    match entry.target with
    | none => true
    | some target => decide (target < symbolCount)

theorem resolution_snapshot_admitted_iff (referenceCount symbolCount : Nat)
    (entries : List ResolutionEntry) :
    resolutionSnapshotAdmitted referenceCount symbolCount entries = true ↔
      entries.length = referenceCount ∧
      ∀ entry ∈ entries, ∀ target, entry.target = some target → target < symbolCount := by
  simp only [resolutionSnapshotAdmitted, Bool.and_eq_true, decide_eq_true_eq,
    List.all_eq_true]
  constructor
  · rintro ⟨length, bounded⟩
    refine ⟨length, ?_⟩
    intro entry member target hasTarget
    have accepted := bounded entry member
    simp [hasTarget] at accepted
    exact accepted
  · rintro ⟨length, bounded⟩
    refine ⟨length, ?_⟩
    intro entry member
    cases hasTarget : entry.target with
    | none => rfl
    | some target =>
      exact decide_eq_true (bounded entry member target hasTarget)

theorem admitted_snapshot_has_exact_length (referenceCount symbolCount : Nat)
    (entries : List ResolutionEntry)
    (accepted : resolutionSnapshotAdmitted referenceCount symbolCount entries = true) :
    entries.length = referenceCount :=
  (resolution_snapshot_admitted_iff referenceCount symbolCount entries).mp accepted |>.1

theorem admitted_target_is_bounded (referenceCount symbolCount : Nat)
    (entries : List ResolutionEntry)
    (accepted : resolutionSnapshotAdmitted referenceCount symbolCount entries = true)
    (entry : ResolutionEntry) (member : entry ∈ entries) (target : Nat)
    (hasTarget : entry.target = some target) : target < symbolCount :=
  (resolution_snapshot_admitted_iff referenceCount symbolCount entries).mp accepted |>.2
    entry member target hasTarget

theorem wrong_length_rejected (referenceCount symbolCount : Nat)
    (entries : List ResolutionEntry) (wrong : entries.length ≠ referenceCount) :
    resolutionSnapshotAdmitted referenceCount symbolCount entries = false := by
  simp [resolutionSnapshotAdmitted, wrong]

theorem out_of_range_target_rejected (referenceCount symbolCount : Nat)
    (entries : List ResolutionEntry) (entry : ResolutionEntry) (member : entry ∈ entries)
    (target : Nat) (hasTarget : entry.target = some target) (outside : symbolCount ≤ target) :
    resolutionSnapshotAdmitted referenceCount symbolCount entries = false := by
  apply Bool.eq_false_iff.mpr
  intro accepted
  have bounded := admitted_target_is_bounded referenceCount symbolCount entries accepted
    entry member target hasTarget
  omega

theorem empty_snapshot_admitted : resolutionSnapshotAdmitted 0 0 [] = true := by
  decide

end FrKernels.ProjectIdentity
