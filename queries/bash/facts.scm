; Bash fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Scoping. Bash is DYNAMICALLY scoped. Every name is global unless declared
; `local`, and a `local` stays visible to whatever the function calls. Function
; bodies are still captured as scopes below, because that is the only lexical
; structure there is, but scope-based resolution in bash is a heuristic: a name
; used inside a function may at run time refer to a caller's variable.
;
; Visibility. Bash has no visibility control over functions — every function is
; global once its definition has run, and one defined in a sourced file is
; visible to the sourcing script. @export is therefore reserved for variables
; declared with `export`, which is the one real visibility distinction the
; language makes (visible to child processes).

(program) @scope
(function_definition body: (compound_statement) @scope)
(subshell) @scope

; Both `f() { ... }` and `function f { ... }` carry a `name` field, so one
; pattern covers both spellings.
(function_definition
  name: (word) @name) @definition.function

; `export X=1`, `local x=1`, `readonly X=1`, `declare -i n=0`. The declaration
; keyword is an anonymous child; `"export"?` matches every declaration form but
; captures @export only for the keyword that actually changes visibility.
(declaration_command
  "export"? @export
  (variable_assignment
    name: (variable_name) @name) @definition.variable)

; `export X` / `local x` — declared with no value. Here the variable name is a
; direct child of the declaration, so the whole declaration is the definition.
(declaration_command
  "export"? @export
  (variable_name) @name) @definition.variable

; A plain `X=1`, in every position an assignment can occupy — except as the
; child of a declaration_command, which the two patterns above already define.
; Listing the parents is what keeps `local x=1` from being defined twice: a
; pattern rooted at (variable_assignment) alone would match there too, and
; tree-sitter queries cannot say "whose parent is not a declaration". The
; grammar's `_statement` supertype is hidden, so it cannot stand in for the
; statement-position parents either.
(program (variable_assignment name: (variable_name) @name) @definition.variable)
(compound_statement (variable_assignment name: (variable_name) @name) @definition.variable)
(subshell (variable_assignment name: (variable_name) @name) @definition.variable)
(command_substitution (variable_assignment name: (variable_name) @name) @definition.variable)
(do_group (variable_assignment name: (variable_name) @name) @definition.variable)
(if_statement (variable_assignment name: (variable_name) @name) @definition.variable)
(elif_clause (variable_assignment name: (variable_name) @name) @definition.variable)
(else_clause (variable_assignment name: (variable_name) @name) @definition.variable)
(while_statement (variable_assignment name: (variable_name) @name) @definition.variable)
(case_item (variable_assignment name: (variable_name) @name) @definition.variable)
(list (variable_assignment name: (variable_name) @name) @definition.variable)
(pipeline (variable_assignment name: (variable_name) @name) @definition.variable)
(negated_command (variable_assignment name: (variable_name) @name) @definition.variable)
(redirected_statement (variable_assignment name: (variable_name) @name) @definition.variable)

; `a=1 b=2` as a single statement.
(variable_assignments (variable_assignment name: (variable_name) @name) @definition.variable)

; `FOO=bar cmd`: an assignment scoped to one command's environment.
(command (variable_assignment name: (variable_name) @name) @definition.variable)

; `for i in ...` binds its loop variable. The identifier is captured as the
; definition too, so full_span stays off the loop body — which would otherwise
; look like it contains, and so owns, every symbol defined inside it.
(for_statement
  variable: (variable_name) @name @definition.variable)

(c_style_for_statement
  initializer: (variable_assignment
    name: (variable_name) @name) @definition.variable)

; A command invocation. Most command names are external programs; the ones that
; name a function defined in the workspace resolve to it in the index.
(command
  name: (command_name (word) @reference.call))

; Every `$X`, `${X}`, `${X[i]}` and arithmetic `x` is a variable_name node, and
; so is the left-hand side of an assignment — the extractor drops a reference
; whose span coincides with a definition's name, so this single catch-all covers
; all use sites without turning definitions into references.
;
; `$1`, `$2`, … are variable_names too, but they are positional parameters: they
; have no definition site and no rename can touch them, so they are excluded
; and not reported as uses of a variable named "1". `$@`, `$?` and friends
; are special_variable_name nodes and never reach this pattern.
((variable_name) @reference.identifier
 (#not-match? @reference.identifier "^[0-9]+$"))

; `source lib.sh` and `. lib.sh` splice another script's definitions into this
; one — bash's only import mechanism. The path is a bare word, a quoted string
; or a concatenation such as `"$DIR"/lib.sh`. The extractor unquotes a path by
; trimming quote characters off its ends, which a concatenation survives only
; imperfectly — the span is exact either way.
((command
   name: (command_name (word) @_cmd)
   argument: (word) @import.path) @import
 (#any-of? @_cmd "source" "."))

((command
   name: (command_name (word) @_cmd)
   argument: (string) @import.path) @import
 (#any-of? @_cmd "source" "."))

((command
   name: (command_name (word) @_cmd)
   argument: (raw_string) @import.path) @import
 (#any-of? @_cmd "source" "."))

((command
   name: (command_name (word) @_cmd)
   argument: (concatenation) @import.path) @import
 (#any-of? @_cmd "source" "."))
