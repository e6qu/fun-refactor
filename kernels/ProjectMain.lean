import FrKernels.Workspace

open FrKernels.Project

def samples : List Nat := [0, 1, 2, 3, 4, 79, 80, 499, 500, 65536, 4294967295, 18446744073709551615]

def main (args : List String) : IO Unit := do
  if args == ["membership"] || args == ["closure"] then
    for size in [0:4] do
      let nodes := List.range size
      let allEdges := nodes.flatMap (fun source => nodes.map (fun target => (source, target)))
      for seedMask in [0:2^size] do
        let seeds := nodes.filter (fun node => seedMask / 2^node % 2 == 1)
        for edgeMask in [0:2^(size*size)] do
          let edges := allEdges.zipIdx.filterMap (fun (edge, index) =>
            if edgeMask / 2^index % 2 == 1 then some edge else none)
          if args == ["closure"] then
            IO.println (workspaceClosure seeds edges)
          else
            for rounds in [0:size+2] do
              IO.println (membershipRounds seeds edges rounds)
    let seeds := [4294967295, 7, 7]
    let edges := [(7, 18446744073709551615), (7, 3), (7, 3), (3, 7), (99, 2)]
    if args == ["closure"] then
      IO.println (workspaceClosure seeds edges)
      for size in [4, 16, 64] do
        let edges := (List.range size).map (fun node => (node, node + 1))
        IO.println (workspaceClosure [0] edges)
    else
      for rounds in [0:4] do
        IO.println (membershipRounds seeds edges rounds)
  else if args == ["confidence"] then
    let mut paths : List (List Nat) := [[]]
    let mut words := paths
    for _ in [0:6] do
      words := words.flatMap (fun stem => [0, 1, 2, 3].map (fun rank => stem ++ [rank]))
      paths := paths ++ words
    for path in paths do
      IO.println (pathConfidence path)
  else if args == ["patterns"] then
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
