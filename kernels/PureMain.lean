import FrKernels.PureKernel
import FrKernels.FormalPlan

def main : IO Unit := do
  for fuel in [0, 1, 64, 256, 257] do
    for environment in [0, 64, 65] do
      for nodes in [0, 4096, 4097] do
        for depth in [0, 64, 65] do
          let admitted := FrPureKernel.limitsAdmitted fuel environment nodes depth
          if admitted != FrKernels.FormalPlan.kernelLimitsAdmitted fuel environment nodes depth then
            throw (IO.userError "semantic and source-bound limit policies disagree")
          IO.println s!"{admitted}"
