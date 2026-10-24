# Keep customer text when cleanup runs

A helper accepts a customer's message and returns it to its caller.
It also calls `finish()` before leaving. In this example, `finish()` records that cleanup ran and returns the status text `PUBLIC`.
The application supplies that operation; the analysis uses an explicit contract for it.

The helper contains a bug:

```python
def prepare(value):
    try:
        return value
    finally:
        return finish()
```

`try` starts a protected section. Python runs the following `finally` section when that section finishes.
This includes leaving with a return, raising an exception, or leaving a loop with `break` or `continue`.
An exception is a signal that interrupts normal execution, such as `ValueError` when an operation rejects a value.

Here Python first evaluates `value` and remembers it as the pending return value.
It then runs cleanup. The second `return` replaces the remembered value with `PUBLIC`.
The customer receives a status message instead of their text.

The repair keeps cleanup and removes its return:

```python
def prepare(value):
    try:
        return value
    finally:
        finish()
```

Now `prepare('Customer')` returns `Customer` and calls `finish()` once.
If cleanup itself raises, that exception still leaves the helper. The repair must not hide it.
The [independent behavior check](../tests/fixtures/python-finally/oracle.py) checks four labels, cleanup counts and a cleanup failure.

## Ask whether the customer text reaches its output

The [starting files](../tests/fixtures/python-finally/) include this caller:

```python
import delivery

def entry():
    return send(delivery.prepare(read_user()))
```

`read_user()` provides the customer's text. `send(value)` is the output we want to check.
The application supplies those operations too. This example declares their behavior rather than analyzing their implementations.
Save the two application files as `app.py` and `delivery.py`, and save this `rules.json` beside them:

```json
{"version":"finally","sources":["read_user"],"sinks":["send"],"propagators":["finish"],"sanitizers":{}}
```

A source introduces the value being followed. A sink is a place that receives it.
A propagator keeps possible connections from its arguments; `finish()` has none here.
The rules do not prove that an external operation succeeds or has no other effects.
The `version` text identifies this caller-supplied rule set.

Find the caller:

```sh
fr --json project find entry --in app.py
```

Copy its returned handle, an identifier for that function in the current project revision.
Use that exact value for `<ENTRY_HANDLE>`:

```sh
fr --json project dataflow '<ENTRY_HANDLE>' --summaries --imports \
  --standard-library builtins --rules rules.json --context html \
  --steps 4096 --depth 8 --bytes 262144
```

`--summaries` follows supported calls by describing each helper's arguments, returns and explicit raises.
`--imports` follows the original local import of `delivery`.
`--standard-library builtins` selects the standard CPython builtins model explicitly.
`--context html` names the output context; it does not make this text safe to insert into HTML.
The last three settings cap analysis work, call depth and report bytes. An exhausted limit leaves an incomplete answer.

Before the repair, the answer contains no customer-text connection to `send`: cleanup replaces the return.
After the repair, the answer includes a possible connection through `prepare`.
A possible connection is a derivation in the stated model, not proof that the program can execute that path without an exception.

## Check and deliver the repair

Use the [authoring instructions](../skills/fr/references/author.md) to preview the replacement body for `prepare`.
A preview shows the proposed edit before changing source.
Use the [delivery instructions](../skills/fr/references/workflow.md) to apply that reviewed change, run the declared checks, undo and redo it, and export a patch.
A patch describes the changed lines so they can be applied in a different checkout.

The [automated repair check](../sdk/python/tests/test_finally_flow.py) exercises this route through `fr`.
It first requires the original program to fail the independently written customer-text check.
It then replaces only `prepare`, verifies cleanup behavior, checks that the saved analysis expires, and applies the patch in another directory.
These are scripted checks on an authored example. They do not measure unfamiliar-agent adoption or time savings.

## Understand the boundaries

Cleanup can preserve or replace a normal return or explicit raise.
The model also follows cleanup when a supported local helper explicitly raises, and when a supported loop breaks or continues.
Nested protected sections keep their pending values separate.
Reassigning a local name during cleanup does not change a value already captured by `return`.

Exception matching with `except`, bare `raise` without a new value, exception groups and `with` context managers remain unsupported.
So do shared mutable objects, such as two names referring to a list that either can change.
For example, a division by zero can interrupt a program even when the scalar model follows its ordinary value path.
The scalar model follows simple values, such as strings and numbers; it does not reproduce every runtime operation.
The report keeps that arithmetic limit visible and does not claim runtime exception safety.

The [25 independent examples](../tests/agent-eval/python-finally/cases.json) compare both Python outcomes and model answers.
They repeat after renaming functions and moving the source file, and include unsupported behavior and altered saved-answer controls.
The existing 512-block graph limit and analysis budgets remain in force. These checks make no speed or memory claim.
