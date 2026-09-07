import FrKernels.Workspace
import FrKernels.Git

open FrKernels.Project

def samples : List Nat := [0, 1, 2, 3, 4, 79, 80, 499, 500, 65536, 4294967295, 18446744073709551615]

def main (args : List String) : IO Unit := do
  if args == ["body-replacement-budget"] then
    for before in [0, 1, 2, 3, 65535, 65536, 65537, 18446744073709551615] do
      for after in [0, 1, 2, 3, 65535, 65536, 65537, 18446744073709551615] do
        IO.println (bodyReplacementBudget before after)
  else if args == ["worktree-archive-compaction"] then
    for complete in [false, true] do
      for checkoutAbsent in [false, true] do
        for metadataAbsent in [false, true] do
          for unlocked in [false, true] do
            IO.println (FrKernels.Git.worktreeArchiveCompactionAllowed complete checkoutAbsent metadataAbsent unlocked)
  else if args == ["worktree-branch-selection"] then
    for existing in [false, true] do
      for present in [false, true] do
        for occupied in [false, true] do
          IO.println (FrKernels.Git.worktreeBranchSelectionAllowed existing present occupied)
  else if args == ["worktree-removal-resume"] then
    for present in [false, true] do
      for identityMatches in [false, true] do
        for bytesMatch in [false, true] do
          for modeMatches in [false, true] do
            IO.println (FrKernels.Git.worktreeRemovalResumeAllowed present identityMatches bytesMatch modeMatches)
  else if args == ["worktree-removal"] then
    for identityMatches in [false, true] do
      for bytesMatch in [false, true] do
        for modeMatches in [false, true] do
          IO.println (FrKernels.Git.worktreeRemovalFileAllowed identityMatches bytesMatch modeMatches)
  else if args == ["worktree-recovery"] then
    for present in [false, true] do
      for bytesMatch in [false, true] do
        for modeMatches in [false, true] do
          IO.println (FrKernels.Git.worktreeRecoveryFileAllowed present bytesMatch modeMatches)
  else if args == ["worktree-budget"] then
    for files in [0, 1, 19999, 20000, 20001, 18446744073709551615] do
      for bytes in [0, 268435455, 268435456, 268435457, 18446744073709551615] do
        for blobBytes in [0, 33554431, 33554432, 33554433, 18446744073709551615] do
          IO.println (FrKernels.Git.worktreeBudgetAllows files bytes blobBytes)
  else if args == ["commit-basis"] then
    for expectedBranch in ["refs/heads/main", "refs/heads/other", "名"] do
      for observedBranch in ["refs/heads/main", "refs/heads/other", "名"] do
        for expectedParent in [none, some "aa", some "bb"] do
          for observedParent in [none, some "aa", some "bb"] do
            IO.println (FrKernels.Git.commitBasisMatches expectedBranch observedBranch expectedParent observedParent)
  else if args == ["staging-transition"] then
    for before in [false, true] do
      for after in [false, true] do
        for recovery in [false, true] do
          IO.println (FrKernels.Git.stagingTransitionAllowed before after recovery)
  else if args == ["call-selection"] then
    for incoming in [false, true] do
      for outgoing in [false, true] do
        for includeIncoming in [false, true] do
          for includeOutgoing in [false, true] do
            IO.println (FrKernels.Git.callInSelection incoming outgoing includeIncoming includeOutgoing)
  else if args == ["line-ranges"] then
    for start in samples do
      for finish in samples do
        for line in samples do
          IO.println (FrKernels.Git.lineInRange start finish line)
  else if args == ["membership"] || args == ["closure"] then
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
