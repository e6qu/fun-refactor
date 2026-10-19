# Check a small function that runs during import

You want to follow submitted input through a helper to an output.
Before the helper runs, Python imports its file and executes the statements outside its functions.
That startup work can change which helper your program calls.
`fr` must check it before calling the answer complete.

A registry is a dictionary holding values or functions for later use.
A registration function fills that dictionary.
`fr` can check a small registration function that writes to dictionaries created earlier in the same file.
The function takes no parameters, and the startup call supplies no arguments.

## Create and check the example

You need Python 3, an installed `fr` executable and its matching Python SDK.
The SDK lets a Python script ask `fr` questions.
Follow the [installation instructions](../README.md#install) and [client setup](../README.md#give-fr-to-an-agent).
Create a new directory and save the following files there.

Save this as `helpers.py`.
`table` starts as an empty dictionary.
The call to `register()` fills it during import by visiting the entries in `labels`.
At each visit, `key` receives the entry name and `item` receives its value.
Here there is one entry: the loop copies the key `label` and the value `ready` into `table`.
The stored `lambda` creates a function; creating it does not run its body.
Here, `missing()` deliberately names an unavailable function to demonstrate that the body stays deferred.

```python
table = {}
labels = {'label': 'ready'}

def register():
    for key, item in labels.items():
        table[key] = item
    table.update(send=lambda: missing())

register()

def relay(value):
    return value
```

Save this as `app.py`.
Importing `relay` first runs the startup statements in `helpers.py`.
`read_input` and `send` stand for external input and output functions.
The launcher below supplies their implementations outside the analyzed files.

```python
from helpers import relay

def entry():
    return send(relay(read_input()))
```

Save this as `rules.json`.
These rules identify input and output without assuming other calls remove or carry the input.

```json
{
  "version": "startup-registration",
  "sources": ["read_input"],
  "sinks": ["send"],
  "sanitizers": {},
  "propagators": []
}
```

Save this as `check_example.py`.
The client finds `entry` and obtains its handle, an identifier for that function.
`summaries=True` follows supported helper calls; `imports=True` includes supported local imports.
The cache keeps the answer in memory.
The step and answer-size limits bound the analysis and its output.

```python
from pathlib import Path

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.runtime import FrClient

client = FrClient('.')
target = client.project('find', 'entry', '--in', 'app.py').definition_target().handle
answer = FlowCache(MemoryObjectStore()).analyze(
    client, target, rules=Path('rules.json'), summaries=True, imports=True,
    steps=4096, max_bytes=1048576,
)
print('Finished:', answer.report.at('/complete'))
print('Possible input connection:', bool(answer.witnesses))
```

Run `python3 check_example.py`. It prints `Finished: True` and `Possible input connection: True`.
Read both lines together: an unfinished answer cannot establish whether the program has an input connection.

Save this as `run_example.py` to run the same application in ordinary Python.
The first two lines of output show that registration ran and stored the callback without calling it.
The last line shows the value returned by the application.

```python
import app
import helpers

app.read_input = lambda: 17
app.send = lambda value: value
print(helpers.table['label'])
print(callable(helpers.table['send']))
print(app.entry())
```

Run `python3 run_example.py`. It prints `ready`, `True` and `17` on separate lines.

## Change startup so it replaces the helper

Add these two lines inside `register`, after its existing writes:

```python
    global relay
    relay = lambda value: 0
```

Also move the `register()` call to the end of `helpers.py`, after the definition of `relay`.
Otherwise that later definition would replace the startup assignment again.
The `global` statement makes the assignment change the file's helper rather than a name local to `register`.

Run both commands again.
Python now prints `ready`, `True` and `0`.
The analysis prints `Finished: False`: the startup call includes an unsupported helper replacement.
Any reported input connection in that unfinished answer still needs investigation.
An empty connection would also leave the question unanswered.

## What the check covers

The checker examines the whole registration function within the existing 128-node syntax limit.
A syntax node represents a piece of parsed code, such as a call, name or literal.
It accepts supported dictionary writes, `pass`, and an optional final `return` or `return None`.
At least one dictionary write must occur.
Keys and stored values follow the [callback-table rules](python-contract.md#prepare-a-callback-table-without-calling-its-entries).
Use fresh names such as `table` and `register`, with basic English letters, digits and underscores.
Neither name can already belong to something else in the file.

The checker rechecks dictionary ownership and built-in names at each call.
An intervening import, unknown call or assignment giving either object another name discards that checked connection.
The existing saved-answer cache records the files read and the places searched for imports; changing them requires another check.
You can also [save startup checks separately](save-python-startup.md). Reuse requires matching source files, import searches and settings; an edit requires another check.

A registration function can also copy a short, known set of entries.
A literal writes values directly in source, such as `labels = {'state': 'ready'}`.
For example, declare `labels = {'state': 'ready'}` before `register`, then put
`for key, item in labels.items():` and the indented write `table[key] = item` inside it.
Python visits each key and its value; here it copies `'state': 'ready'` into `table`.
The source dictionary must come from a literal in the same file and have no intervening writes.
The destination must be a different dictionary created in the same file.
The checker also accepts a nonempty literal tuple or list, such as
`for key in ('state', 'label'):` followed by `table[key] = 'ready'`.
Nested loops need distinct local names.
Both the function's syntax and the work after repeating each loop body must fit 128 nodes.
An empty loop, reused target name, loop `else`, early exit or changed source dictionary stays unfinished.
This includes overwriting an existing source entry: the checker deliberately requires an unchanged source.
Dictionary constructors such as `dict(state='ready')` cannot supply iteration entries yet.

Imported shared dictionaries, helper replacement, nested calls, unknown iterables, conditions and calls with arguments stay unfinished.
So do functions using an `@` decorator or declared with `async def`.
A decorator can replace a function while Python defines it; an asynchronous function needs a caller that awaits its work.
Reading dictionary contents or calling stored callbacks remains outside this rule.
All file, source-size, execution and time limits remain unchanged.
Toolz's registration function also inspects function arguments, imports modules dynamically and computes new values. Checking bounded iteration alone cannot finish its package analysis.

Tests compare these examples and unsafe changes with ordinary Python execution.
This measures agreement on specific behavior, not speed or complete package support.
The extra analysis time and memory for this rule have not been measured separately.
