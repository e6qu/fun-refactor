import FrKernels.Edit

open FrKernels

def sources := ["", "a", "ab", "abc", "abcd", "é", "aé", "🙂"]

def replacements := ["", "X", "YZ", "λ"]

def editsFor (source : String) : List Edit :=
  (List.range (source.utf8ByteSize + 1)).flatMap fun start =>
    (List.range (source.utf8ByteSize - start + 1)).flatMap fun width =>
      replacements.map fun replacement => { start, stop := start + width, replacement }

def plansFor (source : String) : List (List Edit) :=
  let edits := editsFor source
  ([] :: edits.map (fun edit => [edit])) ++
    (edits.flatMap fun first => edits.map fun second => [first, second]) ++
      [[{ start := source.utf8ByteSize, stop := source.utf8ByteSize + 1, replacement := "X" }]]

def main : IO Unit :=
  for source in sources do
    for edits in plansFor source do
      match applyChecked source edits with
      | some output => IO.println s!"ok\t{output}"
      | none => IO.println "reject"
