# Trace a call that replaces a Python default

You want to know whether submitted input reaches an output through a helper.
The helper normally uses a stored object when its caller leaves out an argument.
Your call supplies a replacement, so that stored object never supplies the result.

A parameter is a named slot in a function definition.
A default supplies that slot when the caller omits its argument.
For example, `choose(value=stored)` stores `stored` when Python defines `choose`.
Calling `choose(17)` replaces it with 17; calling `choose()` uses the stored object.
`fr` can trace explicit replacements of checked defaults, including collections and functions.
Consuming those object defaults remains unfinished.

## Create the example

You need Python 3, an installed `fr` executable and its matching Python SDK.
Follow the [installation guide](../README.md#install) and [Python client setup](../README.md#give-fr-to-an-agent).
Create a new directory and run the following commands there.

Save this as `helpers.py`. The empty list is a collection that can change.
Creating it and storing it as a default do not call another function.

```python
stored = []

def choose(value=stored):
    return value
```

Save this as `app.py`. `read_input` and `send` represent external input and output functions.
The rules describe their roles. The launcher below supplies their Python implementations.
Keep those implementations outside `app.py` and its imports; external rules cannot overlap local definitions.

```python
from helpers import choose

def entry():
    return send(choose(read_input()))
```

Save this as `rules.json`. The empty sanitizer rules make no assumptions about calls removing the input connection.
An empty propagator list adds no assumptions about other calls passing input into their results.
The rule label identifies this configuration.

```json
{
  "version": "default-replacement",
  "sources": ["read_input"],
  "sinks": ["send"],
  "sanitizers": {},
  "propagators": []
}
```

## Check the call

Save this as `check_example.py`. The client finds `entry` and gets its handle, an identifier for the selected function.
The cache keeps the answer in memory. `summaries=True` follows supported helper calls; `imports=True` includes supported local imports.
The step limit bounds analysis work. The byte limit caps the structured answer at 1 MiB, or 1,048,576 bytes.

```python
from pathlib import Path

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.runtime import FrClient

client = FrClient(".")
target = client.project("find", "entry", "--in", "app.py").definition_target().handle
answer = FlowCache(MemoryObjectStore()).analyze(
    client, target, rules=Path("rules.json"), summaries=True, imports=True,
    steps=4096, max_bytes=1048576,
)
print("Finished:", answer.report.at("/complete"))
print("Input reaches output:", bool(answer.witnesses))
print("Unfinished reasons:", answer.report.at("/cutoffs"))
```

Run `python3 check_example.py`. It prints `Finished: True`, `Input reaches output: True` and an empty reason list.
The supplied input reaches `send`; the stored list supplies no value to this call.

Save this as `run_example.py` to run the same program in Python.
Assigning the functions to `app` makes those names available when `entry()` runs.
Neither analyzed file imports this launcher.

```python
import app

def read_input():
    return 17

def send(value):
    return value

app.read_input = read_input
app.send = send
print(app.entry())
```

Run `python3 run_example.py`. It prints 17.

## Remove the replacement

Change `choose(read_input())` to `choose()` in `app.py`, then run both commands again.
Python prints `[]`, because this call uses the stored list.
The analysis prints `Finished: False` and includes `unsupported-default-value:helpers.py::choose` in its reasons.
An empty input-to-output trace in that unfinished answer cannot establish that a real program is safe.

Changing the helper to `def choose(value=make_default()): ...` creates a different problem.
Python calls `make_default` while defining the function, before the later replacement argument exists.
Supplying an argument cannot remove that earlier work. Unknown calls, missing names and computed defaults keep the analysis unfinished.

Stored functions, classes, supported instances and checked imported values follow the same replacement rule.
Calls through function parameters, changing shared objects and reading arbitrary global values keep their existing limits.
Mixed defaults work independently: omitted checked scalars remain supported, while each stored object needs its own replacement.
The file, source-size, step and elapsed-time limits remain unchanged.

The tests compare supported traces and unfinished controls with Python execution.
They do not establish complete package support, lower agent cost or faster analysis.
The extra checking and memory use of this rule have not been measured separately.
