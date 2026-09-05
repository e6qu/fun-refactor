import FrKernels.Project

open FrKernels.Project

def samples : List Nat := [0, 1, 2, 3, 4, 79, 80, 499, 500, 65536, 4294967295, 18446744073709551615]

def main (args : List String) : IO Unit := do
  if args == ["patterns"] then
    let alphabet := ["a", "b", "*", "λ", "", "a/b"]
    let mut paths : List (List String) := [[]]
    let mut words := paths
    for _ in [0:3] do
      words := words.flatMap (fun stem => alphabet.map (fun part => stem ++ [part]))
      paths := paths ++ words
    for pattern in paths do
      for path in paths do
        IO.println (workspacePatternMatches pattern path)
  else
    for total in samples do
      for start in samples do
        for limit in samples do
          IO.println (pageLength total start limit)
