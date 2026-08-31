-- Lean has no hook that runs on the way out of a scope, so the order is written.
def work : IO Unit := do
  IO.println "open a"
  IO.println "open b"
  IO.println "work"
  IO.println "close b"
  IO.println "close a"

def main : IO Unit := do
  work
  IO.println "done"
