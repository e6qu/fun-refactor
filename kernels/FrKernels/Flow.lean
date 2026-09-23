namespace FrKernels.Flow

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

def joinMask (left right : Nat) : Nat :=
  ((List.range 4).filter fun bit => join (maskFacts left) (maskFacts right) bit).foldl
    (fun sum bit => sum + 2 ^ bit) 0

end FrKernels.Flow
