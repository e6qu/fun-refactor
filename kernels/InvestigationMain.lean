import FrKernels.Investigation
import FrKernels.Flow
open FrKernels.Investigation

def main (args : List String) : IO Unit := do
  if args == ["local-module"] then
    for source in [false, true] do
      for packageMissing in [false, true] do
        for stubMissing in [false, true] do
          IO.println (localModuleAdmitted source packageMissing stubMissing)
    return
  if args == ["proof-evidence"] then
    for executed in [false, true] do
      for stable in [false, true] do
        for checked in [false, true] do
          for debtFree in [false, true] do
            IO.println (proofAcceptable executed stable checked debtFree)
    return
  if args == ["compiler-coverage"] then
    for execution in [false, true] do
      for protocol in [false, true] do
        for fromStart in [false, true] do
          for noRemaining in [false, true] do
            IO.println (compilerEvidenceComplete execution protocol fromStart noRemaining)
    return
  if args == ["flow-fact-coverage"] then
    for analysis in [false, true] do
      for fromStart in [false, true] do
        for noRemaining in [false, true] do
          IO.println (FrKernels.Flow.factComplete analysis fromStart noRemaining)
    return
  if args == ["flow-summary"] then
    for useFirst in [false, true] do
      for useSecond in [false, true] do
        for first in List.range 16 do
          for second in List.range 16 do
            IO.println (FrKernels.Flow.transferMask first second useFirst useSecond)
    return
  if args == ["check-scope"] then
    for workspace in [false, true] do
      for configuration in [false, true] do
        for sources in [false, true] do
          for toolchain in [false, true] do
            IO.println (checkScopeCovered workspace configuration sources toolchain)
    return
  if args == ["flow-join"] then
    for left in List.range 16 do
      for right in List.range 16 do
        for boundLeft in [false, true] do
          for boundRight in [false, true] do
            let joined := FrKernels.Flow.joinMask left right
            let bound := boundLeft && boundRight
            IO.println s!"{joined} {bound} {joined != left || bound != boundLeft}"
    return
  let states := [State.pending, .ready, .running, .satisfied, .blocked, .stale]
  for initial in states do
    for target in states do
      for prerequisites in [false, true] do
        for evidence in [false, true] do
          IO.println (transitionAllowed initial target prerequisites evidence)
