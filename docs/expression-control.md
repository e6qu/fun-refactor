# Ordered Python expression analysis

Opt-in summary analysis evaluates scalar `and`, `or`, `not`, conditional expressions and comparison
chains in Python operand order. Each operand is transferred once per expression evaluation; the
fixed-point solver can evaluate a function repeatedly. Effects come only from admitted operands.

```python
False and sink(source())          # The right operand is skipped.
value if flag else stop(value)    # Unknown flag retains normal and raising alternatives.
left() < middle() < right()       # middle runs once; right is conditional.
```

Boolean and `None` literals provide known truth values. Parentheses, `not` and selected nested
expressions preserve that information. Variables, call results and numeric comparisons stay unknown.
Unknown selectors retain both alternatives without asserting they are feasible together. For example,
`1 < 0 < sink(source())` can retain a possible sink effect because comparison values are not solved.
Zero, strings and other literal values are also not specialized by this transfer layer.

A Boolean operator returns its selected operand's origins. A conditional expression returns the
selected branches' origins, without adding the selector as an implicit flow. An unknown branch
that raises or does not return cannot erase another branch's normal continuation. Explicit raise
and sink effects join across admitted branches. A comparison retains the existing conservative
operand-origin propagation and may stop after any completed comparison. Its first two operands are
mandatory. Unavailable summary returns remain a fixed-point approximation, not a termination proof.

The contract assumes scalar truth and comparison operations. Overloaded `__bool__`, `__len__`,
comparison protocols, implicit exceptions, handlers, heap effects and general path feasibility are
outside it. An evaluated unsupported expression or exhausted budget reports incomplete analysis.
An expression skipped by a known selector needs no execution contract. Lexical binding checks and
module-closure checks still apply independently, including to source containing skipped expressions.

Expression control was introduced in `python-scalar-summaries-2`; current reports use version 5
with [call binding](call-binding.md) and [ordered assignments](scalar-assignments.md).
They include `fr-expression-control-1` in both the public
report and canonical inputs. Analyzer identities include the expression implementation. Native reuse
rejects changed contracts even with a recomputed retained-report digest. The Python SDK exposes
`FunctionSummaries.expression_control` and rejects malformed or inconsistent contracts. It still
reads earlier summary records, where this field is `None`; that compatibility does not authorize
reusing an old report under the new analyzer.

The [pinned task](../tests/agent-eval/expression-control/task.json) exercises unknown-target repair,
feature delivery, resumption, reversal and independent receiver replay. The [runtime corpus](../sdk/python/tests/test_expression_control.py)
checks finite positive and negative cases against CPython. These observations establish their stated
outcomes, not general source/model correspondence.

[Retained acceptance](../tests/agent-eval/results/2026-09-29-expressions-expression-control/result.json)
binds the evaluator, implementation and fixture. Both delivered outcomes pass independent receiver
replay. Ordinary discovery reads 471 source bytes across six files; native discovery returns 128,738
bytes across eight calls with no source-body reveals. These finite measurements establish no general
efficiency or token-saving benefit.
