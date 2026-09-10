import Init.Data.List.Sort.Lemmas

namespace FrKernels.Project

-- fr:spec src/project/framework_kernel.rs::framework_emitted @ 9afa46708862e53eb40bf7e4c5f732cf57c9007874e05a1efe18fe61d5b80ac7
-- fr:signature total: usize => total: Nat; limit: usize => limit: Nat; return: usize => return: Nat
def frameworkEmitted (total : Nat) (limit : Nat) : Nat := min total limit

-- fr:spec src/project/framework_kernel.rs::framework_omitted @ 1fcd250b558f64c8203739f82829b3958874173d42694554d312c42e5ecf4f32
-- fr:signature total: usize => total: Nat; limit: usize => limit: Nat; return: usize => return: Nat
def frameworkOmitted (total : Nat) (limit : Nat) : Nat := total - limit

theorem framework_partition (total limit : Nat) :
    frameworkEmitted total limit + frameworkOmitted total limit = total := by
  by_cases ordered : total ≤ limit
  · simp [frameworkEmitted, frameworkOmitted, Nat.min_eq_left ordered,
      Nat.sub_eq_zero_of_le ordered]
  · have reverse : limit ≤ total := Nat.le_of_not_ge ordered
    rw [frameworkEmitted, frameworkOmitted, Nat.min_eq_right reverse]
    omega

theorem framework_emitted_respects_limit (total limit : Nat) :
    frameworkEmitted total limit ≤ limit := Nat.min_le_right _ _

theorem framework_omitted_is_zero_iff (total limit : Nat) :
    frameworkOmitted total limit = 0 ↔ total ≤ limit := by
  unfold frameworkOmitted
  omega

-- fr:spec src/project/framework_kernel.rs::middleware_request_order @ 8216192cd608316f86f46581f9d759b435f3aa05e1ccb7a3aae1597b1dbf4027
-- fr:signature total: usize => total: Nat; declaration_index: usize => declarationIndex: Nat; return: usize => return: Nat
def middlewareRequestOrder (total : Nat) (declarationIndex : Nat) : Nat := total - declarationIndex

theorem middleware_request_order_in_range (total declarationIndex : Nat)
    (valid : declarationIndex < total) :
    1 ≤ middlewareRequestOrder total declarationIndex ∧
      middlewareRequestOrder total declarationIndex ≤ total := by
  simp [middlewareRequestOrder]
  omega

theorem middleware_request_order_reverses (total earlier later : Nat)
    (ordered : earlier < later) (valid : later < total) :
    middlewareRequestOrder total later < middlewareRequestOrder total earlier := by
  simp [middlewareRequestOrder]
  omega

-- fr:spec src/project/framework_kernel.rs::component_hooks_compatible @ 2683af30799c8796c086e3f5c4a01a644ab0cda68bed70ff4bb13fce8d5eaa7b
-- fr:signature client: bool => client: Bool; runtime_hooks: usize => runtimeHooks: Nat; return: bool => return: Bool
def componentHooksCompatible (client : Bool) (runtimeHooks : Nat) : Bool :=
  client || decide (runtimeHooks = 0)

theorem server_hooks_compatible_iff_empty (runtimeHooks : Nat) :
    componentHooksCompatible false runtimeHooks = true ↔ runtimeHooks = 0 := by
  simp [componentHooksCompatible]

theorem client_hooks_are_compatible (runtimeHooks : Nat) :
    componentHooksCompatible true runtimeHooks = true := by
  simp [componentHooksCompatible]

-- fr:spec src/project/framework_kernel.rs::configuration_visibility @ 537d173c9f9187b1a7ce38245dfe63887d1e24f8fc54db9b88b0bec4a59a7195
-- fr:signature nextjs: bool => nextjs: Bool; public_name: bool => publicName: Bool; return: usize => return: Nat
def configurationVisibility (nextjs : Bool) (publicName : Bool) : Nat :=
  if !nextjs then 0 else if publicName then 2 else 1

theorem configuration_visibility_is_known (nextjs publicName : Bool) :
    configurationVisibility nextjs publicName ≤ 2 := by
  cases nextjs <;> cases publicName <;> decide

theorem public_configuration_requires_next (nextjs publicName : Bool) :
    configurationVisibility nextjs publicName = 2 ↔ nextjs = true ∧ publicName = true := by
  cases nextjs <;> cases publicName <;> decide

-- fr:spec src/project/framework_kernel.rs::service_target_kind @ 2c124fe8abc7f90d35c6be9bbb18102239701d2de79f51d882fbd7a94f5a3e4b
-- fr:signature absolute_http: bool => absoluteHttp: Bool; root_relative: bool => rootRelative: Bool; return: usize => return: Nat
def serviceTargetKind (absoluteHttp : Bool) (rootRelative : Bool) : Nat :=
  if absoluteHttp then 2 else if rootRelative then 1 else 0

theorem service_target_kind_is_known (absoluteHttp rootRelative : Bool) :
    serviceTargetKind absoluteHttp rootRelative ≤ 2 := by
  cases absoluteHttp <;> cases rootRelative <;> decide

theorem absolute_service_target_wins (rootRelative : Bool) :
    serviceTargetKind true rootRelative = 2 := by
  cases rootRelative <;> rfl

-- fr:spec src/project/framework_kernel.rs::service_redaction_flags @ ab640c3628e3562292ae80a5bb5e809068215aa9a001eb1542ba4a72abaa99cd
-- fr:signature query_or_fragment: bool => queryOrFragment: Bool; credentials: bool => credentials: Bool; return: usize => return: Nat
def serviceRedactionFlags (queryOrFragment : Bool) (credentials : Bool) : Nat :=
  (if queryOrFragment then 1 else 0) + (if credentials then 2 else 0)

theorem service_redaction_flags_are_bounded (queryOrFragment credentials : Bool) :
    serviceRedactionFlags queryOrFragment credentials ≤ 3 := by
  cases queryOrFragment <;> cases credentials <;> decide

theorem service_redaction_zero_iff_clear (queryOrFragment credentials : Bool) :
    serviceRedactionFlags queryOrFragment credentials = 0 ↔
      queryOrFragment = false ∧ credentials = false := by
  cases queryOrFragment <;> cases credentials <;> decide

-- fr:spec src/project/framework_kernel.rs::fastapi_prefix_supported @ c6a0e76ee26cc51b3ff32c70f42e44e45eda7ada1ce40bd9fa9db0f9f2e1be78
-- fr:signature empty: bool => empty: Bool; starts_slash: bool => startsSlash: Bool; ends_slash: bool => endsSlash: Bool; return: bool => return: Bool
def fastapiPrefixSupported (empty : Bool) (startsSlash : Bool) (endsSlash : Bool) : Bool :=
  empty || startsSlash && !endsSlash

theorem empty_fastapi_prefix_is_supported (startsSlash endsSlash : Bool) :
    fastapiPrefixSupported true startsSlash endsSlash = true := by
  cases startsSlash <;> cases endsSlash <;> decide

theorem nonempty_fastapi_prefix_supported_iff (startsSlash endsSlash : Bool) :
    fastapiPrefixSupported false startsSlash endsSlash = true ↔
      startsSlash = true ∧ endsSlash = false := by
  cases startsSlash <;> cases endsSlash <;> decide

theorem trailing_slash_rejects_nonempty_fastapi_prefix (startsSlash : Bool) :
    fastapiPrefixSupported false startsSlash true = false := by
  cases startsSlash <;> decide

-- fr:spec src/project/framework_kernel.rs::framework_migration_supported @ 2b6adb54c00914716834b06d5d8f08020911833f96c16fc10cd7ca9f621d3e8f
-- fr:signature source_fastapi: bool => sourceFastapi: Bool; target_fastapi: bool => targetFastapi: Bool; return: bool => return: Bool
def frameworkMigrationSupported (sourceFastapi : Bool) (targetFastapi : Bool) : Bool :=
  sourceFastapi != targetFastapi

theorem framework_migration_supported_iff_crosses_boundary
    (sourceFastapi targetFastapi : Bool) :
    frameworkMigrationSupported sourceFastapi targetFastapi = true ↔
      sourceFastapi != targetFastapi := by
  cases sourceFastapi <;> cases targetFastapi <;> decide

-- fr:spec src/project/framework_kernel.rs::migration_disposition @ 12d6711dd5abc9fdfd1c6c94fee86397941097d6444a2edc0f3bde8898ce147c
-- fr:signature gap: bool => gap: Bool; automatic_kind: bool => automaticKind: Bool; return: usize => return: Nat
def migrationDisposition (gap : Bool) (automaticKind : Bool) : Nat :=
  if gap then 2 else if automaticKind then 0 else 1

theorem migration_gap_is_unsupported (automaticKind : Bool) :
    migrationDisposition true automaticKind = 2 := by
  cases automaticKind <;> decide

theorem supported_automatic_kind_is_automatic :
    migrationDisposition false true = 0 := by
  decide

theorem supported_nonautomatic_kind_needs_a_decision :
    migrationDisposition false false = 1 := by
  decide

-- fr:spec src/project/framework_kernel.rs::migration_schema_agreement @ 1b6293851270d68ca599ab29dc38f9687619f8af10f01955a0eb5f1371072a03
-- fr:signature expected: &[String] => expected: List String; generated: &[String] => generated: List String; return: bool => return: Bool
def migrationSchemaAgreement (expected : List String) (generated : List String) : Bool :=
  expected.all (generated.contains ·)

theorem migration_schema_agreement_iff_subset (expected generated : List String) :
    migrationSchemaAgreement expected generated = true ↔
      ∀ shape ∈ expected, shape ∈ generated := by
  simp [migrationSchemaAgreement]

theorem migration_schema_agreement_reflexive (shapes : List String) :
    migrationSchemaAgreement shapes shapes = true := by
  simp [migrationSchemaAgreement]

-- fr:spec src/project/framework_kernel.rs::nextjs_registration_automatic @ 1b11251331a535bbe2f7ecebe93bc4d6ea8937469de89d774eff03ef0db87db0
-- fr:signature declares_next: bool => declaresNext: Bool; app_router_path: bool => appRouterPath: Bool; return: bool => return: Bool
def nextjsRegistrationAutomatic (declaresNext : Bool) (appRouterPath : Bool) : Bool :=
  declaresNext && appRouterPath

theorem nextjs_registration_automatic_iff_evidence
    (declaresNext appRouterPath : Bool) :
    nextjsRegistrationAutomatic declaresNext appRouterPath = true ↔
      declaresNext = true ∧ appRouterPath = true := by
  cases declaresNext <;> cases appRouterPath <;> decide

theorem nextjs_registration_requires_dependency (appRouterPath : Bool) :
    nextjsRegistrationAutomatic false appRouterPath = false := by
  cases appRouterPath <;> decide

-- fr:spec src/project/framework_kernel.rs::fastapi_body_parameter_automatic @ 917a5d5e70786d7d2692a97244b5e5d3ba9cf1601a671a80bd78a3e28c81fa87
-- fr:signature candidate_count: usize => candidateCount: Nat; path_collision: bool => pathCollision: Bool; query_collision: bool => queryCollision: Bool; return: bool => return: Bool
def fastapiBodyParameterAutomatic
    (candidateCount : Nat) (pathCollision : Bool) (queryCollision : Bool) : Bool :=
  decide (candidateCount = 1) && !pathCollision && !queryCollision

theorem fastapi_body_parameter_automatic_iff_unique_without_collision
    (candidateCount : Nat) (pathCollision queryCollision : Bool) :
    fastapiBodyParameterAutomatic candidateCount pathCollision queryCollision = true ↔
      candidateCount = 1 ∧ pathCollision = false ∧ queryCollision = false := by
  cases pathCollision <;> cases queryCollision <;> simp [fastapiBodyParameterAutomatic]

theorem fastapi_body_parameter_rejects_path_collision
    (candidateCount : Nat) (queryCollision : Bool) :
    fastapiBodyParameterAutomatic candidateCount true queryCollision = false := by
  cases queryCollision <;> simp [fastapiBodyParameterAutomatic]

theorem fastapi_body_parameter_rejects_query_collision
    (candidateCount : Nat) (pathCollision : Bool) :
    fastapiBodyParameterAutomatic candidateCount pathCollision true = false := by
  cases pathCollision <;> simp [fastapiBodyParameterAutomatic]

-- fr:spec src/project/framework_kernel.rs::nextjs_body_validation_automatic @ 22ec1ca1fa70fe1f5ab3eaca2645ba66a6221b87683a4fe8f50b8baea0cb329e
-- fr:signature candidate_count: usize => candidateCount: Nat; supported_shape: bool => supportedShape: Bool; return: bool => return: Bool
def nextjsBodyValidationAutomatic (candidateCount : Nat) (supportedShape : Bool) : Bool :=
  decide (candidateCount = 1) && supportedShape

theorem nextjs_body_validation_automatic_iff_unique_supported
    (candidateCount : Nat) (supportedShape : Bool) :
    nextjsBodyValidationAutomatic candidateCount supportedShape = true ↔
      candidateCount = 1 ∧ supportedShape = true := by
  cases supportedShape <;> simp [nextjsBodyValidationAutomatic]

theorem nextjs_body_validation_rejects_unsupported (candidateCount : Nat) :
    nextjsBodyValidationAutomatic candidateCount false = false := by
  simp [nextjsBodyValidationAutomatic]

theorem nextjs_body_validation_accepts_unique_supported :
    nextjsBodyValidationAutomatic 1 true = true := by
  decide

-- fr:spec src/project/framework_kernel.rs::fastapi_registration_automatic @ 5a48b67679e7d69d6375ea7924f45d6df5de6355043b63c35ec02940d20547b4
-- fr:signature explicit_target: bool => explicitTarget: Bool; application_binding: bool => applicationBinding: Bool; endpoint_conflict: bool => endpointConflict: Bool; return: bool => return: Bool
def fastapiRegistrationAutomatic
    (explicitTarget : Bool) (applicationBinding : Bool) (endpointConflict : Bool) : Bool :=
  explicitTarget && applicationBinding && !endpointConflict

theorem fastapi_registration_automatic_iff_explicit_valid_without_conflict
    (explicitTarget applicationBinding endpointConflict : Bool) :
    fastapiRegistrationAutomatic explicitTarget applicationBinding endpointConflict = true ↔
      explicitTarget = true ∧ applicationBinding = true ∧ endpointConflict = false := by
  cases explicitTarget <;> cases applicationBinding <;> cases endpointConflict <;> decide

theorem fastapi_registration_rejects_implicit
    (applicationBinding endpointConflict : Bool) :
    fastapiRegistrationAutomatic false applicationBinding endpointConflict = false := by
  cases applicationBinding <;> cases endpointConflict <;> decide

theorem fastapi_registration_rejects_endpoint_conflict
    (explicitTarget applicationBinding : Bool) :
    fastapiRegistrationAutomatic explicitTarget applicationBinding true = false := by
  cases explicitTarget <;> cases applicationBinding <;> decide

-- fr:spec src/project.rs::path_confidence @ b5a8549e
-- fr:signature edges: &[Confidence] => edges: List Nat; return: Confidence => return: Nat
def pathConfidence (edges : List Nat) : Nat := edges.foldr max 0

theorem path_confidence_empty : pathConfidence [] = 0 := rfl

theorem path_confidence_cannot_strengthen (edges : List Nat) (edge : Nat)
    (member : edge ∈ edges) : edge ≤ pathConfidence edges := by
  induction edges with
  | nil => simp at member
  | cons head tail ih =>
    change edge ≤ max head (pathConfidence tail)
    rcases List.mem_cons.mp member with same | rest
    · subst edge
      exact Nat.le_max_left _ _
    · exact Nat.le_trans (ih rest) (Nat.le_max_right _ _)

theorem path_confidence_stays_in_tiers (edges : List Nat) (ceiling : Nat)
    (bounded : ∀ edge ∈ edges, edge ≤ ceiling) : pathConfidence edges ≤ ceiling := by
  induction edges with
  | nil => exact Nat.zero_le _
  | cons head tail ih =>
    change max head (pathConfidence tail) ≤ ceiling
    exact Nat.max_le.mpr ⟨bounded head (by simp), ih (fun edge member => bounded edge (by simp [member]))⟩

-- fr:spec src/project.rs::page_length @ b4a90c73
-- fr:signature total: usize => total: Nat; start: usize => start: Nat; limit: usize => limit: Nat; return: usize => return: Nat
def pageLength (total : Nat) (start : Nat) (limit : Nat) : Nat :=
  min (total - start) limit

theorem page_respects_limit (total start limit : Nat) : pageLength total start limit ≤ limit := by
  exact Nat.min_le_right _ _

theorem page_stays_in_result (total start limit : Nat) (valid : start ≤ total) :
    start + pageLength total start limit ≤ total := by
  unfold pageLength
  omega

theorem page_advances (total start limit : Nat) (remaining : start < total) (positive : 0 < limit) :
    start < start + pageLength total start limit := by
  unfold pageLength
  omega

theorem page_and_remaining_partition (total start limit : Nat) (valid : start ≤ total) :
    pageLength total start limit + (total - (start + pageLength total start limit)) = total - start := by
  unfold pageLength
  omega

theorem beyond_end_is_empty (total start limit : Nat) (ended : total ≤ start) :
    pageLength total start limit = 0 := by
  unfold pageLength
  omega

-- fr:spec src/project.rs::workspace_pattern_matches @ 3166b1cc
-- fr:signature pattern: &[String] => pattern: List String; path: &[String] => path: List String; return: bool => return: Bool
def workspacePatternMatches (pattern : List String) (path : List String) : Bool :=
  match pattern, path with
  | [], [] => true
  | p :: ps, part :: parts => (p == "*" || p == part) && workspacePatternMatches ps parts
  | _, _ => false

theorem matches_preserve_depth (pattern path : List String)
    (h : workspacePatternMatches pattern path = true) : pattern.length = path.length := by
  induction pattern generalizing path with
  | nil => cases path <;> simp_all [workspacePatternMatches]
  | cons p ps ih =>
    cases path with
    | nil => simp [workspacePatternMatches] at h
    | cons part parts =>
      simp only [workspacePatternMatches, Bool.and_eq_true] at h
      simpa using congrArg Nat.succ (ih parts h.2)

theorem different_depth_refuses (pattern path : List String)
    (h : pattern.length ≠ path.length) : workspacePatternMatches pattern path = false := by
  cases result : workspacePatternMatches pattern path with
  | false => rfl
  | true => exact False.elim (h (matches_preserve_depth pattern path result))

theorem literal_path_matches_itself (path : List String) : workspacePatternMatches path path = true := by
  induction path with
  | nil => rfl
  | cons part parts ih => simp [workspacePatternMatches, ih]

theorem matched_head_is_literal_or_star (p part : String) (ps parts : List String)
    (h : workspacePatternMatches (p :: ps) (part :: parts) = true) : p = "*" ∨ p = part := by
  simp only [workspacePatternMatches, Bool.and_eq_true] at h
  simpa using h.1

def canonicalMembers (members : List Nat) : List Nat :=
  (members.foldr List.insert []).mergeSort (· ≤ ·)

theorem canonical_membership (members : List Nat) (node : Nat) :
    node ∈ canonicalMembers members ↔ node ∈ members := by
  simp only [canonicalMembers, List.mem_mergeSort]
  induction members with
  | nil => simp
  | cons head tail ih => simp [ih]

-- fr:spec src/project.rs::workspace_membership_step @ 5436711f
-- fr:signature members: &[usize] => members: List Nat; edges: &[(usize, usize)] => edges: List (Nat × Nat); return: Vec<usize> => return: List Nat
def workspaceMembershipStep (members : List Nat) (edges : List (Nat × Nat)) : List Nat :=
  canonicalMembers (members ++ (edges.filter (fun edge => edge.1 ∈ members)).map Prod.snd)

theorem membership_step_iff (members : List Nat) (edges : List (Nat × Nat)) (node : Nat) :
    node ∈ workspaceMembershipStep members edges ↔
      node ∈ members ∨ ∃ source, (source, node) ∈ edges ∧ source ∈ members := by
  simp [workspaceMembershipStep, canonical_membership, List.mem_map, List.mem_filter,
    Prod.exists, and_assoc]

theorem membership_step_preserves (members : List Nat) (edges : List (Nat × Nat))
    (node : Nat) (present : node ∈ members) : node ∈ workspaceMembershipStep members edges :=
  (membership_step_iff members edges node).mpr (Or.inl present)

theorem membership_step_monotone (smaller larger : List Nat) (edges : List (Nat × Nat))
    (included : ∀ node ∈ smaller, node ∈ larger) :
    ∀ node ∈ workspaceMembershipStep smaller edges, node ∈ workspaceMembershipStep larger edges := by
  intro node present
  apply (membership_step_iff larger edges node).mpr
  rcases (membership_step_iff smaller edges node).mp present with old | ⟨source, edge, known⟩
  · exact Or.inl (included node old)
  · exact Or.inr ⟨source, edge, included source known⟩

def membershipRounds (seeds : List Nat) (edges : List (Nat × Nat)) : Nat → List Nat
  | 0 => canonicalMembers seeds
  | rounds + 1 => workspaceMembershipStep (membershipRounds seeds edges rounds) edges

inductive MemberReachable (seeds : List Nat) (edges : List (Nat × Nat)) : Nat → Prop
  | seed {node} : node ∈ seeds → MemberReachable seeds edges node
  | edge {source target} : MemberReachable seeds edges source →
      (source, target) ∈ edges → MemberReachable seeds edges target

theorem membership_rounds_sound (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat) :
    ∀ node ∈ membershipRounds seeds edges rounds, MemberReachable seeds edges node := by
  induction rounds with
  | zero =>
    intro node present
    exact .seed ((canonical_membership seeds node).mp present)
  | succ rounds ih =>
    intro node present
    rcases (membership_step_iff _ edges node).mp present with old | ⟨source, edge, known⟩
    · exact ih node old
    · exact .edge (ih source known) edge

theorem membership_rounds_preserve_seeds (seeds : List Nat) (edges : List (Nat × Nat))
    (rounds node : Nat) (seed : node ∈ seeds) : node ∈ membershipRounds seeds edges rounds := by
  induction rounds with
  | zero => exact (canonical_membership seeds node).mpr seed
  | succ rounds ih => exact membership_step_preserves _ edges node ih

theorem reachable_in_closed_superset (seeds : List Nat) (edges : List (Nat × Nat))
    (allowed : Nat → Prop) (includes : ∀ node ∈ seeds, allowed node)
    (closed : ∀ source target, allowed source → (source, target) ∈ edges → allowed target)
    (node : Nat) (reachable : MemberReachable seeds edges node) : allowed node := by
  induction reachable with
  | seed present => exact includes _ present
  | edge _ edge ih => exact closed _ _ ih edge

theorem membership_rounds_stay_in_closed_superset (seeds : List Nat) (edges : List (Nat × Nat))
    (allowed : Nat → Prop) (includes : ∀ node ∈ seeds, allowed node)
    (closed : ∀ source target, allowed source → (source, target) ∈ edges → allowed target)
    (rounds node : Nat) (present : node ∈ membershipRounds seeds edges rounds) : allowed node :=
  reachable_in_closed_superset seeds edges allowed includes closed node
    (membership_rounds_sound seeds edges rounds node present)

theorem stabilized_membership_is_exact (seeds : List Nat) (edges : List (Nat × Nat)) (rounds : Nat)
    (stable : workspaceMembershipStep (membershipRounds seeds edges rounds) edges =
      membershipRounds seeds edges rounds) (node : Nat) :
    node ∈ membershipRounds seeds edges rounds ↔ MemberReachable seeds edges node := by
  constructor
  · exact membership_rounds_sound seeds edges rounds node
  · apply reachable_in_closed_superset seeds edges (fun n => n ∈ membershipRounds seeds edges rounds)
    · exact fun n h => membership_rounds_preserve_seeds seeds edges rounds n h
    · intro source target known edge
      rw [← stable]
      exact (membership_step_iff _ edges target).mpr (Or.inr ⟨source, edge, known⟩)

-- fr:spec src/project.rs::body_replacement_budget @ aab2e9e5858491c8ab262936994086babac8d14f8ae79fecd5e36b96897d65b3
-- fr:signature before: usize => before: Nat; after: usize => after: Nat; return: bool => return: Bool
def bodyReplacementBudget (before : Nat) (after : Nat) : Bool :=
  decide (1 ≤ before ∧ before ≤ 65536 ∧ 1 ≤ after ∧ after ≤ 65536)

theorem body_replacement_bounds (before after : Nat)
    (accepted : bodyReplacementBudget before after = true) :
    1 ≤ before ∧ before ≤ 65536 ∧ 1 ≤ after ∧ after ≤ 65536 := by
  simpa [bodyReplacementBudget] using accepted

theorem body_replacement_budget_is_symmetric (before after : Nat) :
    bodyReplacementBudget before after = bodyReplacementBudget after before := by
  simp [bodyReplacementBudget, and_comm, and_left_comm, and_assoc]

end FrKernels.Project
