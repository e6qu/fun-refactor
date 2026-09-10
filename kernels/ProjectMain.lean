import FrKernels.Workspace
import FrKernels.Git
import FrKernels.Source
import FrKernels.Author
import FrKernels.Adoption
import FrKernels.Checks

open FrKernels.Project

def samples : List Nat := [0, 1, 2, 3, 4, 79, 80, 499, 500, 65536, 4294967295, 18446744073709551615]

def frameworkSamples : List Nat := [0, 1, 2, 63, 64, 65, 128, 512, 65536]

def schemaSamples : List (List String) := [[], ["a"], ["a", "b"], ["b", "a"], ["a", "a"]]

def sourceSamples : List String := Id.run do
  let mut sources := [""]
  let mut words := sources
  for _ in [0:3] do
    words := words.flatMap (fun stem => ["a", "é", "名", "🙂"].map (stem ++ ·))
    sources := sources ++ words
  return sources ++ [String.ofList [Char.ofNat 0, '\r', '\n'],
    String.ofList ([0x7f, 0x80, 0x7ff, 0x800, 0xffff, 0x10000, 0x10ffff].map Char.ofNat),
    "é", String.ofList [Char.ofNat 0xfeff], "\t\\\""]

def sourceBudgets : List Nat := List.range 17 ++ [65536, 4294967295, 18446744073709551615]

def declarationOffsetSamples : List String := Id.run do
  let mut sources := [""]
  let mut words := sources
  for _ in [0:4] do
    words := words.flatMap (fun stem => ["a", " ", "\t", "\r", "\n", "é", "🙂"].map (stem ++ ·))
    sources := sources ++ words
  return sources ++ [String.ofList ['\n', Char.ofNat 0xa0], String.ofList ['\n', Char.ofNat 0x2003],
    String.ofList ['\n', Char.ofNat 0x2028], String.ofList ['\n', Char.ofNat 11],
    String.ofList ['\n', Char.ofNat 12], "mod target {\r\n  ", "/* } */ ", "\n// }", "\r\n\t\r ",
    String.ofList [Char.ofNat 0, '\n', ' ']]

def declarationOffsetLargeSamples : List String :=
  [String.ofList (List.replicate 4096 '🙂') ++ "{\n\t  ",
   "{\r\n" ++ String.ofList (List.replicate 65536 ' '),
   "{" ++ String.ofList (List.replicate 4096 ' ') ++ "x"]

def main (args : List String) : IO Unit := do
  if args == ["framework-boundaries"] then
    for total in frameworkSamples do
      for limit in frameworkSamples do
        IO.println (frameworkEmitted total limit)
        IO.println (frameworkOmitted total limit)
    for total in frameworkSamples do
      for declarationIndex in frameworkSamples do
        IO.println (middlewareRequestOrder total declarationIndex)
    for client in [false, true] do
      for runtimeHooks in [0, 1, 2, 65536] do
        IO.println (componentHooksCompatible client runtimeHooks)
    for nextjs in [false, true] do
      for publicName in [false, true] do
        IO.println (configurationVisibility nextjs publicName)
    for absoluteHttp in [false, true] do
      for rootRelative in [false, true] do
        IO.println (serviceTargetKind absoluteHttp rootRelative)
    for queryOrFragment in [false, true] do
      for credentials in [false, true] do
        IO.println (serviceRedactionFlags queryOrFragment credentials)
    for empty in [false, true] do
      for startsSlash in [false, true] do
        for endsSlash in [false, true] do
          IO.println (fastapiPrefixSupported empty startsSlash endsSlash)
    for sourceFastapi in [false, true] do
      for targetFastapi in [false, true] do
        IO.println (frameworkMigrationSupported sourceFastapi targetFastapi)
    for gap in [false, true] do
      for automaticKind in [false, true] do
        IO.println (migrationDisposition gap automaticKind)
    for expected in schemaSamples do
      for generated in schemaSamples do
        IO.println (migrationSchemaAgreement expected generated)
    for declaresNext in [false, true] do
      for appRouterPath in [false, true] do
        IO.println (nextjsRegistrationAutomatic declaresNext appRouterPath)
    for candidateCount in [0, 1, 2, 65536] do
      for pathCollision in [false, true] do
        for queryCollision in [false, true] do
          IO.println (fastapiBodyParameterAutomatic candidateCount pathCollision queryCollision)
    for candidateCount in [0, 1, 2, 65536] do
      for supportedShape in [false, true] do
        IO.println (nextjsBodyValidationAutomatic candidateCount supportedShape)
    for explicitTarget in [false, true] do
      for applicationBinding in [false, true] do
        for endpointConflict in [false, true] do
          IO.println (fastapiRegistrationAutomatic explicitTarget applicationBinding endpointConflict)
    for explicitCutover in [false, true] do
      for registrationAutomatic in [false, true] do
        for externalReferences in [false, true] do
          IO.println (migrationCutoverAutomatic explicitCutover registrationAutomatic externalReferences)
    for pep621Manifest in [false, true] do
      for ownsDestination in [false, true] do
        for dependenciesArray in [false, true] do
          for requirementsCoverMissing in [false, true] do
            IO.println (migrationDependencyEditAutomatic pep621Manifest ownsDestination
              dependenciesArray requirementsCoverMissing)
    for executed in [false, true] do
      for commandsPassed in [false, true] do
        for configurationStable in [false, true] do
          for sourceSnapshotStable in [false, true] do
            IO.println (FrKernels.Checks.checkEvidenceAcceptable
              executed commandsPassed configurationStable sourceSnapshotStable)
    for requirementPresent in [false, true] do
      for configurationMatches in [false, true] do
        for checkNamesMatch in [false, true] do
          IO.println (FrKernels.Checks.checkRequirementSatisfied
            requirementPresent configurationMatches checkNamesMatch)
  else if args == ["selection-conflicts"] then
    for leftStart in samples do
      for leftEnd in samples do
        if leftStart ≤ leftEnd then
          for rightStart in samples do
            for rightEnd in samples do
              if rightStart ≤ rightEnd then
                IO.println (FrKernels.Author.selectionConflict leftStart leftEnd rightStart rightEnd)
  else if args == ["declaration-offsets"] then
    for text in declarationOffsetSamples do
      let starts := List.range (FrKernels.Source.byteLength text.toList + 2) ++ [4294967295, 18446744073709551615]
      for bodyStart in starts do
        IO.println (FrKernels.Author.declarationInsertionOffset text bodyStart)
    for text in declarationOffsetLargeSamples do
      for bodyStart in [0, 1, FrKernels.Source.byteLength text.toList - 1,
          FrKernels.Source.byteLength text.toList, 18446744073709551615] do
        IO.println (FrKernels.Author.declarationInsertionOffset text bodyStart)
  else if let ["declaration-offset", bodyStart, text] := args then
    IO.println (FrKernels.Author.declarationInsertionOffset text bodyStart.toNat!)
  else if args == ["source-slices"] then
    for text in sourceSamples do
      let offsets := List.range (FrKernels.Source.byteLength text.toList + 2) ++ [4294967295, 18446744073709551615]
      for offset in offsets do
        for budget in sourceBudgets do
          match FrKernels.Source.sourceSliceLength text offset budget with
          | none => IO.println "none"
          | some length => IO.println length
  else if args == ["source-pages"] then
    let mut pages : List (List String) := [[]]
    let mut words := pages
    for _ in [0:3] do
      words := words.flatMap (fun stem => ["", "a", "é", "名", "🙂", "a🙂é"].map (stem ++ [·]))
      pages := pages ++ words
    for page in pages do
      for budget in sourceBudgets do
        IO.println (FrKernels.Source.sourcePageLengths page budget)
  else if let "source-page" :: budget :: texts := args then
    IO.println (FrKernels.Source.sourcePageLengths texts budget.toNat!)
  else if args == ["body-replacement-budget"] then
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
  else if args == ["worktree-configuration"] then
    for reviewedMode in [false, true] do
      for observedMode in [false, true] do
        for configPresent in [false, true] do
          for configRegular in [false, true] do
            IO.println (FrKernels.Git.worktreeConfigurationAllowed reviewedMode observedMode configPresent configRegular)
  else if args == ["worktree-prepared-recovery"] then
    for receiptPresent in [false, true] do
      for preparationMatches in [false, true] do
        for registrationMatches in [false, true] do
          IO.println (FrKernels.Git.worktreePreparedRecoveryAllowed receiptPresent preparationMatches registrationMatches)
  else if args == ["worktree-entry-mode"] then
    for regular in [false, true] do
      for executable in [false, true] do
        for symlink in [false, true] do
          for objectIsBlob in [false, true] do
            IO.println (FrKernels.Git.worktreeEntryModeAllowed regular executable symlink objectIsBlob)
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
  else if args == ["staging-record-compaction"] then
    for detailed in [false, true] do
      for pending in [false, true] do
        for retained in [false, true] do
          IO.println (FrKernels.Git.stagingRecordCompactable detailed pending retained)
  else if args == ["staging-crash-state"] then
    for pending in [false, true] do
      for indexLock in [false, true] do
        for preparation in [false, true] do
          IO.println (FrKernels.Git.stagingCrashStateRequiresReview pending indexLock preparation)
  else if args == ["staging-index-entry"] then
    for stageZero in [false, true] do
      for intentToAdd in [false, true] do
        for assumeUnchanged in [false, true] do
          for skipWorktree in [false, true] do
            IO.println (FrKernels.Git.stagingIndexEntryReplayable stageZero intentToAdd assumeUnchanged skipWorktree)
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
  else if args == ["adoption-debt"] then
    for obligations in [0:65] do
      for ceiling in [0:65] do
        IO.println (FrKernels.Adoption.debtWithinCeiling obligations ceiling)
  else
    for total in samples do
      for start in samples do
        for limit in samples do
          IO.println (pageLength total start limit)
