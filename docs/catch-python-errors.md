# Catch the error the library actually raises

An application checks a customer's configured number bounds with Boltons' `clamp` function.
Boltons is a Python utility library. Its original `boltons.mathutils` module supplies `clamp`.
For example, `clamp(70, 0, 50)` returns `50`.
Reversing the bounds makes the function raise `ValueError` instead of returning a number.

An exception interrupts normal execution. Its class, such as `ValueError` or `TypeError`, identifies the kind of error.
A Python `except` clause catches matching exceptions and runs recovery code.
This application catches the wrong class:

```python
from boltons.mathutils import clamp

def prepare(value, lower, upper):
    try:
        clamp(0, lower, upper)
    except TypeError:
        return value
    return 'ACCEPTED'
```

The application checks the configured bounds using a sample number, `0`.
Valid bounds return the status `ACCEPTED`.
When the bounds are reversed, recovery should return the original customer label so the application can show what needs correcting.
Instead, `ValueError` escapes because `except TypeError` does not match it.

The repair changes the clause to `except ValueError`.
It must also preserve unexpected errors: a bound with the wrong type must still raise `TypeError`.

| Input | Required behavior after repair |
|---|---|
| `prepare('Customer', 0, 50)` | Return `ACCEPTED` |
| `prepare('Customer', 50, 0)` | Return `Customer` through recovery |
| `prepare('Customer', 'invalid-bound', 50)` | Let `TypeError` leave the application |

The [selected task](../tests/agent-eval/python-handlers/selection.json) fixes the package revision, behavior and limits.
Boltons was already familiar. This task makes no new unfamiliar-project or time-saving claim.

## Follow the customer label through the original import

Use the original Boltons checkout at revision `55dfe5077bcb5fa76611b5f2a557ea6c442ad87c`.
Place the [starting application](../tests/fixtures/python-handlers/app.py) in its root as `app.py`.
The check uses the real package import; copying `clamp` into the application would test a different program.

The application includes this entry point, the function where analysis begins:

```python
def entry():
    return send(prepare(read_user(), 50, 0))
```

`read_user()` supplies the customer label. `send(value)` receives the output.
The application supplies those operations. Save their declared behavior in `rules.json`:

```json
{"version":"handlers","sources":["read_user"],"sinks":["send"],"sanitizers":{}}
```

A source introduces the value being followed. A sink receives it.
These declarations are assumptions about the application's external operations.
The `version` text identifies this rule set; it does not select an older analyzer format.

Find the entry point:

```sh
fr --json project find entry --in app.py
```

Copy its handle, an identifier tied to the current function and project revision.
Use it in place of `<ENTRY_HANDLE>`:

```sh
fr --json project dataflow '<ENTRY_HANDLE>' --summaries --imports \
  --standard-library builtins,math,bisect,binascii \
  --rules rules.json --context html --steps 4096 --depth 8 --bytes 262144
```

`--summaries` describes helper inputs, returns and explicit raises, then uses those descriptions at each call.
`--imports` follows the original `boltons.mathutils` import.
The standard-library selection names the builtin operations and standard modules used by that module.
`--context html` names the output context; it does not make a customer label safe to insert into HTML.
The remaining settings limit analysis work, call depth and output bytes. Exhausting a limit leaves an incomplete answer.

Before the repair, the supported model finds no customer-label connection through recovery: the raised `ValueError` misses the `TypeError` clause.
After the repair, it finds a possible connection through the matching `ValueError` clause.
This describes the stated model. It does not prove that every runtime operation succeeds or that every possible path can execute.

## Check and deliver the repair

Use the [authoring instructions](../skills/fr/references/author.md) to preview a replacement body for `prepare`.
Change only `except TypeError` to `except ValueError`.
A preview shows the proposed edit before changing source.
Then follow the [delivery instructions](../skills/fr/references/workflow.md) to apply the reviewed edit, run checks, undo and redo it, and export a patch.
A patch records changed lines for application in another checkout.

The [independent behavior check](../tests/fixtures/python-handlers/oracle.py) checks original number bounds, four recovery labels and an unexpected `TypeError`.
The starting program must fail that check.
The [automated repair](../sdk/python/tests/test_exception_handlers.py) uses `fr` for the change and verifies the exported patch in a separate checkout.
It also checks that the Boltons implementation remains unchanged and that the old saved answer expires after the handler edit.
Full checks run on GitHub; follow the [workstation rules](development.md#working-on-a-shared-desktop) for local commands.

## Understand which errors the model follows

Python tests handlers in order and runs the first match.
`except (ValueError, TypeError)` catches either listed class; `except ()` catches neither.
`Exception` also catches `ValueError` and `TypeError`.
`BaseException` catches those and a directly raised `BaseException`, which `Exception` leaves alone.
A bare `except:` catches every explicitly raised error in this model.

An `else` section runs only after the protected body finishes normally.
Errors raised inside `else` or a handler bypass that statement's sibling handlers.
They can still reach an outer handler and run `finally` cleanup on the way.
A bare `raise` inside a supported handler sends the same error onward.
It also works in cleanup while a known error is leaving the protected section.
The handler or cleanup must appear in the current function.
A different helper that relies on its caller's caught error remains unchecked.

Each error class keeps its own possible value origins, the places its value came from.
For example, a helper can raise `ValueError(customer_label)` on one branch and `TypeError('PUBLIC')` on another.
The public helper description keeps those classes and origins separate.
An empty origin map still records a possible error when its class is present.

Custom classes, `except ... as error` bindings, exception groups and shared mutable objects remain unsupported.
The error name in `as error` refers to an exception object, which this simple-value model does not track.
Calls and arithmetic can also raise implicitly, without an explicit `raise` statement.
For example, `int('invalid')` can raise `ValueError`; this extension does not model that runtime failure.
The report continues to state that runtime exception safety is unestablished.
Neither a finished simple-value analysis nor an empty trace proves the absence of all runtime paths or errors.

The [independent examples](../tests/agent-eval/python-handlers/cases.json) cover matching order, helper calls, cleanup, reraises and unsupported cases.
They repeat after renaming functions and moving the source file.
The existing report and work limits remain unchanged.
