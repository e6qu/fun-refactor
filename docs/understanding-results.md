# What can I conclude from a result?

Suppose you ask an agent to fix checkout. A useful answer should tell you where to look,
what the tool checked, what remains uncertain and how to check the proposed repair.

The examples here explain result meanings. They are illustrations, not new measured runs.
A declaration defines a function; a caller invokes it. A handle is a returned reference selecting that code.
A model is the tool's description of supported behavior; execution is what happens when the actual program runs.
The examples distinguish those two kinds of answer.

## “I found the function”

```sh
fr --json project find calculate --signature
fr --json project calls '<HANDLE>' --direction incoming --limit 8
```

Replace `<HANDLE>` with the full reference returned for the selected function. It identifies
the code you just inspected. Ask again if that code changes.

These results help you find the calculation and its known callers. They do not establish
that the calculation is correct. If the list has more pages or reports missing relationships,
read those before treating the caller list as complete.

## “This input may reach that output”

For Python tracing, tell `fr` which calls supply the input and which calls consume it.
For example, `read_input()` could supply a value and `send()` could consume it.
Those descriptions are your assumptions about the external calls.

```python
value = read_input()
send(value)
```

A reported trace connects the input to the output through behavior the analyzer supports.
It is a possible connection in that model. It does not establish an execution that reaches
the output with a particular value.

For example, the analyzer can also retain an input connection here:

```python
value = read_input()
send(value - value)
```

For ordinary numeric inputs, Python sends zero. The analyzer follows where the operands
come from, without calculating that cancellation. Read the relevant code or run a behavior
test before claiming that this report demonstrates a leak.

## “There is no trace in this report”

Suppose the code replaces the input before sending it:

```python
value = read_input()
value = 0
send(value)
```

The supported model can explain why the original input no longer reaches `send`.
An unfinished search, however, can also return an empty list. The empty list alone cannot
answer the question.

For Python dataflow reports, read `complete`, `witnesses` and `cutoffs` together:

| Result | What you can conclude |
| --- | --- |
| `complete: true`, traces present | The supported model finds a possible input-to-output connection. Check its assumptions and the actual behavior. |
| `complete: true`, no traces | The model finds no connection for the configured inputs and outputs, within its stated limits. |
| `complete: false`, with or without traces | The question remains partly unanswered. Read the reasons in `cutoffs`. |
| Command error | The command did not return an analysis answer. Read the error before trying another question. |

Even a complete model does not cover every Python effect. For example, the current model
does not account for every exception that Python can raise. See the [Python examples](python-contract.md)
before using an analysis result as a safety claim.

## “The change is ready to apply”

A preview shows the proposed source changes. Review the whole diff and the files it affects.
The returned review reference connects the write to what you inspected. If the files change,
prepare a fresh preview.

Applying the preview establishes that the reviewed edit reached the files. It does not
establish that checkout now rounds correctly. Require a test that checks the rounding rule,
alongside the build and other relevant checks. [Declare those checks](project-checks.md)
so an agent can inspect and run them.

## “The tests or proof passed”

A passing test establishes what happened for its tested inputs. For example, a zero-value
checkout test cannot establish that fractional prices round correctly.

A proof establishes its stated property under its stated assumptions. A proof about a
translated access rule needs a checked connection to the function you intend to use.
Read [proof support](lean-specs.md) for the supported forms and remaining assumptions.

For current priorities, use the [focused roadmap](../PLAN.md).
The [retained results](roadmap-status.md) audit earlier claims separately. A scripted patch replay
tests delivery; a comparison where an agent discovers the solution tests a different question.
The [product review](product-review.md) explains why we still cannot claim general savings.

## “The benchmark improved”

Ask which operation was measured and what it was compared with.
A smaller reply can save conversation text while still requiring indexing, validation and Python execution.
For example, calling `fr` five times inside Python can send one short result to the agent; the five calls still run.
Read the [comparison and overhead guide](benchmarks.md) before treating a text-size reduction as lower time or cost.
