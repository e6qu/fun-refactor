namespace FrKernels.Flow

def factComplete (analysis fromStart noRemaining : Bool) : Bool :=
  analysis && fromStart && noRemaining

theorem incomplete_analysis_cannot_complete (fromStart noRemaining : Bool) :
    factComplete false fromStart noRemaining = false := by
  simp [factComplete]

theorem omitted_prefix_cannot_complete (analysis noRemaining : Bool) :
    factComplete analysis false noRemaining = false := by
  simp [factComplete]

theorem omitted_suffix_cannot_complete (analysis fromStart : Bool) :
    factComplete analysis fromStart false = false := by
  simp [factComplete]

theorem complete_analysis_and_disclosure : factComplete true true true = true := by
  rfl

abbrev Facts := Nat → Bool

def join (left right : Facts) : Facts := fun origin => left origin || right origin

def assign (environment : Nat → Facts) (target : Nat) (value : Facts) : Nat → Facts :=
  fun name => if name = target then value else environment name

theorem join_idempotent (facts : Facts) : join facts facts = facts := by
  funext origin
  simp [join]

theorem join_commutative (left right : Facts) : join left right = join right left := by
  funext origin
  simp [join, Bool.or_comm]

theorem join_associative (a b c : Facts) : join (join a b) c = join a (join b c) := by
  funext origin
  simp [join, Bool.or_assoc]

theorem join_retains_origin (left right : Facts) (origin : Nat) (h : left origin = true) :
    join left right origin = true := by
  simp [join, h]

theorem overwrite_replaces_origins (environment : Nat → Facts) (target : Nat) (value : Facts) :
    assign environment target value target = value := by
  simp [assign]

theorem overwrite_preserves_other (environment : Nat → Facts) (target name : Nat)
    (value : Facts) (h : name ≠ target) :
    assign environment target value name = environment name := by
  simp [assign, h]

def maskFacts (mask : Nat) : Facts := fun bit => mask / 2 ^ bit % 2 == 1

def transfer (first second : Facts) (useFirst useSecond : Bool) : Facts :=
  fun origin => (useFirst && first origin) || (useSecond && second origin)

theorem transfer_no_parameters (first second : Facts) :
    transfer first second false false = fun _ => false := by
  funext origin
  simp [transfer]

theorem transfer_first (first second : Facts) : transfer first second true false = first := by
  funext origin
  simp [transfer]

theorem transfer_second (first second : Facts) : transfer first second false true = second := by
  funext origin
  simp [transfer]

theorem transfer_both (first second : Facts) : transfer first second true true = join first second := by
  funext origin
  simp [transfer, join]

theorem transfer_monotone (first second widerFirst widerSecond : Facts)
    (useFirst useSecond : Bool)
    (hfirst : ∀ origin, first origin = true → widerFirst origin = true)
    (hsecond : ∀ origin, second origin = true → widerSecond origin = true) :
    ∀ origin, transfer first second useFirst useSecond origin = true →
      transfer widerFirst widerSecond useFirst useSecond origin = true := by
  intro origin h
  simp only [transfer, Bool.or_eq_true, Bool.and_eq_true] at h ⊢
  rcases h with h | h
  · exact Or.inl ⟨h.1, hfirst origin h.2⟩
  · exact Or.inr ⟨h.1, hsecond origin h.2⟩

def transferMask (first second : Nat) (useFirst useSecond : Bool) : Nat :=
  ((List.range 4).filter fun bit => transfer (maskFacts first) (maskFacts second) useFirst useSecond bit).foldl
    (fun sum bit => sum + 2 ^ bit) 0

def joinMask (left right : Nat) : Nat :=
  ((List.range 4).filter fun bit => join (maskFacts left) (maskFacts right) bit).foldl
    (fun sum bit => sum + 2 ^ bit) 0

end FrKernels.Flow
