import FrKernels.Digest

open FrKernels.Digest

def fragment (text : String) : List UInt8 := text.toUTF8.data.toList

def alphabet : List (Operation UInt8) :=
  [.success (fragment "null"), .success (fragment "true"), .success (fragment "\"\""),
   .success (fragment "\"λ🙂\\n\""), .success (fragment "18446744073709551615"),
   .failure (fragment "[\"partial\""), .flush]

def corpus : List (List (Operation UInt8)) := Id.run do
  let mut cases := [[]]
  let mut words := cases
  for _ in [0:3] do
    words := words.flatMap (fun stem => alphabet.map (stem ++ [·]))
    cases := cases ++ words
  for width in [65529, 65530, 65531, 100000] do
    let text := String.ofList (List.replicate width 'a')
    let incomplete := String.ofList (List.replicate 100000 'b')
    cases := cases ++ [[.success (fragment "null"), .success (fragment ("\"" ++ text ++ "\"")),
      .failure (fragment ("[\"" ++ incomplete ++ "\"")), .flush,
      .success (fragment "\"λ🙂\\n\""), .flush, .flush, .success (fragment "null")]]
  return cases

def printState (state : State UInt8) : IO Unit := IO.println s!"[{state.emitted}, {state.pending}]"

def main : IO Unit := do
  for ops in corpus do
    let mut state : State UInt8 := ⟨[], []⟩
    printState state
    for op in ops do
      state := step 65536 state op
      printState state
