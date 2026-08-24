; Sass Local Scope Queries
; =========================

; Function and mixin definitions create scope
(function_statement
  (name) @local.definition.function) @local.scope

(mixin_statement
  (name) @local.definition.function) @local.scope

; Parameters are local definitions
(parameter
  (variable_name) @local.definition.parameter)

(rest_parameter
  (variable_name) @local.definition.parameter)

; Loop variables
(each_statement
  (variable) @local.definition.variable) @local.scope

(for_statement
  (variable_name) @local.definition.variable) @local.scope

; Variable declarations
(declaration
  (variable_name) @local.definition.variable)

; Block creates scope
(block) @local.scope

; Variable references
(variable_value) @local.reference

; With parameters
(with_parameter
  (variable_name) @local.definition.variable)
