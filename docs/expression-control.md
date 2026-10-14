# Which Python expressions actually run?

In `False and send_email(user_input())`, Python skips both calls. A useful trace should preserve that order.
`fr` handles some literal choices and keeps both possibilities when it cannot decide. The details below explain why a possible path can differ from one observed execution.

## Commands, results and limits

Summary analysis follows `and`, `or`, `not`, conditional expressions and comparison chains in
Python's evaluation order. It follows each operand once during one expression evaluation.
It can revisit a function while solving its summary; those visits are analysis work, not runtime calls.

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

If you are investigating an unwanted email, `False and send_email(user_input())` gives a supported
reason to skip that call. An unknown `enabled` flag requires considering both alternatives.
Inspect the flag or run a behavior test before claiming that the reported email path executes.

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

Current reports use `python-scalar-summaries`
with [call binding](call-binding.md) and [ordered assignments](scalar-assignments.md).
They include `fr-expression-control` in both the public
report and canonical inputs. Analyzer identities include the expression implementation. Native reuse
rejects changed contracts even with a recomputed retained-report digest. The Python SDK exposes
`FunctionSummaries.expression_control` and requires its complete current contract.
Other formats fail explicitly; the SDK provides no compatibility adapter.

The [pinned task](../tests/agent-eval/expression-control/task.json) exercises unknown-target repair,
feature delivery, resumption, reversal and independent receiver replay. The [runtime corpus](../sdk/python/tests/test_expression_control.py)
checks finite positive and negative cases against CPython. These observations establish their stated
outcomes, not general source/model correspondence.

[Retained acceptance](../tests/agent-eval/results/2026-09-29-expressions-expression-control/result.json)
binds the evaluator, implementation and fixture. Both delivered outcomes pass independent receiver
replay. Ordinary discovery reads 471 source bytes across six files; native discovery returns 128,738
bytes across eight calls with no source-body reveals. These finite measurements establish no general
efficiency or token-saving benefit.
