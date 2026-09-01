import FrKernels.Position

open FrKernels

def alphabet := ["a", "é", "\n", "名"]

def words : Nat -> List String
  | 0 => [""]
  | size + 1 => (words size).flatMap fun head => alphabet.map fun character => head ++ character

def sources := (List.range 5).flatMap words

def renderOffset (source : String) (offset : Nat) : String :=
  let position := lineCol source offset
  s!"position\t{position.line}\t{position.col}"

def renderPosition (source : String) (line column : Nat) : String :=
  match offsetAt source { line, col := column } with
  | some offset => s!"offset\t{offset}"
  | none => "none"

def main : IO Unit :=
  for source in sources do
    for offset in List.range (source.utf8ByteSize + 3) do
      IO.println (renderOffset source offset)
    for line in List.range 6 do
      for column in List.range 9 do
        IO.println (renderPosition source line column)
