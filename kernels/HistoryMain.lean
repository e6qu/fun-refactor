import FrKernels.History

open FrKernels.History

def samples : List Snapshot :=
  [none, some ⟨"", 384⟩, some ⟨"λ\n", 384⟩, some ⟨"λ\n", 489⟩, some ⟨"名", 420⟩]

def main : IO Unit :=
  for current in samples do
    for before in samples do
      for after in samples do
        for recovery in [false, true] do
          IO.println (matchesSnapshot current before after recovery)
