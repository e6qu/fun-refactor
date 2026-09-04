namespace FrKernels

structure LineCol where
  line : Nat
  col : Nat
  deriving Repr, DecidableEq

structure ByteSpan where
  start : Nat
  stop : Nat
  deriving Repr, DecidableEq

def byteLength : List Char -> Nat
  | [] => 0
  | character :: rest => character.utf8Size + byteLength rest

def lines (characters : List Char) : List (List Char) :=
  go characters [] []
where
  go : List Char -> List Char -> List (List Char) -> List (List Char)
    | [], current, done => (current.reverse :: done).reverse
    | '\n' :: [], current, done => (current.reverse :: done).reverse
    | '\n' :: rest, current, done => go rest [] (current.reverse :: done)
    | character :: rest, current, done => go rest (character :: current) done

def lineColFrom (characters : List Char) (limit bytes : Nat) (position : LineCol) : LineCol :=
  match characters with
  | [] => position
  | character :: rest =>
    if bytes + character.utf8Size <= limit then
      if character == '\n' then
        if rest.isEmpty then position
        else lineColFrom rest limit (bytes + character.utf8Size)
          { line := position.line + 1, col := 1 }
      else lineColFrom rest limit (bytes + character.utf8Size)
        { line := position.line, col := position.col + 1 }
    else position

def lineCol (source : String) (offset : Nat) : LineCol :=
  lineColFrom source.toList (min offset source.utf8ByteSize) 0 { line := 1, col := 1 }

def byteAtColumn : List Char -> Nat -> Nat
  | [], _ => 0
  | _, 0 => 0
  | character :: rest, column + 1 => character.utf8Size + byteAtColumn rest column

theorem byteAtColumn_le_byteLength (characters : List Char) (column : Nat) :
    byteAtColumn characters column ≤ byteLength characters := by
  induction characters generalizing column with
  | nil => simp [byteAtColumn, byteLength]
  | cons character rest ih =>
    cases column with
    | zero => simp [byteAtColumn]
    | succ column =>
      simp [byteAtColumn, byteLength, ih]

def offsetAtLines (all : List (List Char)) (wanted current start column : Nat) : Option Nat :=
  match all with
  | [] => none
  | line :: rest =>
    if wanted == current then some (start + byteAtColumn line (column - 1))
    else offsetAtLines rest wanted (current + 1) (start + byteLength line + 1) column

def offsetAt (source : String) (position : LineCol) : Option Nat :=
  if position.line == 0 then none
  else offsetAtLines (lines source.toList) position.line 1 0 position.col

def trailingNewline (characters : List Char) : Bool :=
  match characters.reverse with
  | '\n' :: _ => true
  | _ => false

def fullLineSpanAt (all : List (List Char)) (trailing : Bool)
    (wanted current start : Nat) : Option ByteSpan :=
  match all with
  | [] => none
  | line :: rest =>
    if wanted == current then
      let stop := start + byteLength line
      some { start, stop := if rest.isEmpty && !trailing then stop else stop + 1 }
    else fullLineSpanAt rest trailing wanted (current + 1) (start + byteLength line + 1)

-- fr:spec src/edit.rs::full_line_span @ 84a16817
-- fr:signature source: &str => source: String; offset: usize => offset: Nat; return: Span => return: ByteSpan
def fullLineSpan (source : String) (offset : Nat) : ByteSpan :=
  match fullLineSpanAt (lines source.toList) (trailingNewline source.toList)
      (lineCol source offset).line 1 0 with
  | some span => span
  | none => { start := 0, stop := 0 }

theorem empty_source_has_one_position (offset : Nat) :
    lineCol "" offset = { line := 1, col := 1 } := by
  simp [lineCol, lineColFrom]

theorem zero_is_not_a_line (source : String) (column : Nat) :
    offsetAt source { line := 0, col := column } = none := by
  simp [offsetAt]

example : lineCol "let名 = \"héllo\";\n" 6 = { line := 1, col := 5 } := by decide
example : lineCol "a\n" 2 = { line := 1, col := 2 } := by decide
example : lineCol "a\n" 3 = { line := 1, col := 2 } := by decide
example : offsetAt "aé\n🙂" { line := 2, col := 2 } = some 8 := by decide
example : offsetAt "a\nb\n" { line := 3, col := 1 } = none := by decide
example : fullLineSpan "a\nb\n" 1 = { start := 0, stop := 2 } := by decide
example : fullLineSpan "a\nb\n" 4 = { start := 2, stop := 4 } := by decide

end FrKernels
