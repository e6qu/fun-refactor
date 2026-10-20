# Ask two Python questions after one startup check

Suppose a program imports two helpers. One passes a value along, and the other
returns zero. Before checking either helper, `fr` needs to check the code Python
runs while importing their files. That is the startup check. This lesson asks both
questions in one request and shares that work.

Create an empty directory and put these four files in it. Use an installed `fr`
and its matching Python SDK, or follow the [SDK setup](python-contract.md).
The SDK is the Python library that calls `fr` and reads its reports.

`helpers.py`:

```python
table = {}


def register():
    table['label'] = 'ready'


register()


def relay(value):
    return value


def discard(value):
    return 0
```

`table` is a dictionary: it associates a key such as `'label'` with a value.
`register()` fills that dictionary during import. Its writes meet the
[small startup-call rules](check-python-startup.md).

`app.py`:

```python
from helpers import relay, discard


def forwards():
    return sink(relay(source()))


def stopped():
    return sink(discard(source()))
```

Here `source()` supplies a value, and `sink(value)` receives an output. The next
file tells `fr` to treat those two names as the input and output of interest.
These rules are assumptions supplied by the caller.

`rules.json`:

```json
{"version":"lesson","sources":["source"],"sinks":["sink"]}
```

`check_example.py`:

```python
from pathlib import Path
from fr_ir.flow_together import analyze_together
from fr_ir.runtime import FrClient

client = FrClient(Path.cwd(), max_output_bytes=1_048_576)
names = ('forwards', 'stopped')
handles = [client.project('find', name).definition_target().handle for name in names]
result = analyze_together(client, handles, standard_library=(), rules=Path('rules.json'),
                          imports=True, steps=4096, max_bytes=1_048_576)
print('Startup checks:', result.report.at('/startup/checks'))
for name, answer in zip(names, result.answers):
    print(name, 'finished:', answer.report.at('/complete'),
          'possible input connection:', bool(answer.witnesses))
```

Run `python check_example.py`. Expected output:

```text
Startup checks: 1
forwards finished: True possible input connection: True
stopped finished: True possible input connection: False
```

A finished answer means `fr` checked the question under its
[published Python rules](python-contract.md).
A possible connection means values may pass from an input to an output.
The checker can report connections that Python would never take.
In the Python API, `answer.witnesses` holds the explanations for those connections.
If an answer is unfinished, an empty connection list does not
establish that the output is safe.

To compare with Python itself, create `run_example.py`:

```python
import builtins

builtins.source = lambda: 17
builtins.sink = lambda value: value

import app
from helpers import table

print(table['label'])
print(app.forwards())
print(app.stopped())
```

Run `python run_example.py`. It prints `ready`, `17` and `0`, each on its own line.
A `lambda` creates a small function from one expression.
`builtins` holds names Python can find without importing them into each file.
These temporary definitions give the example files their input and output functions. The checker does not execute this runner.

## What work is shared

`analyze_together` asks about 2–16 distinct top-level functions in the same file.
Top-level means outside any class or other function.
With `imports=True`, it also follows that file's local imports.
It prepares the declarations and checked startup statements once. Each function then gets its own answer. The command-line
equivalent adds `--also OTHER_FUNCTION_HANDLE` to `fr project dataflow` with
`--summaries`. Function handles are references returned by `find` for the current
file contents; the example obtains them instead of inventing them.

The returned startup record lists checked definition names and the source
locations of checked dictionary startup statements. It shows up to sixteen locations
and counts any others in `dictionary_omitted`. The checker still checks every
statement; the location limit only bounds the displayed detail.
It also binds the full source
files, import candidates (including missing files), configuration, checking rules
and analyzer implementation. A dictionary write inside `register()` is covered
by the recorded call statement and its complete source file.

`analyze_together` shares work for this request. A new call checks startup again.
Use [saved startup preparation](save-python-startup.md) to ask separate questions later.
Whole-answer saved results use `FlowCache`.
For example, editing the registration body, replacing a helper, or adding a
competing `helpers.pyi` file changes the next request's inputs for both answers.
The checker never treats a previous startup check as permission to ignore an edit.

The existing import limits still apply to the one shared set of files: 16 modules,
256 KiB of source, and 128 import searches. A KiB is 1,024 bytes of source text. All questions together share the
requested step limit and output byte limit.
Steps count the work used to trace values through functions.
Finding imports and checking startup have their own existing limits.
When earlier questions use the steps,
later answers remain unfinished. If the combined report will not fit, the command
refuses it. Splitting a large package into subchecks does not grant extra budget.

## Try an unchecked startup effect

Add `missing()` after `register()` in `helpers.py`. Both questions now remain
unfinished because the effects of that call are unknown. Python raises
`NameError` while importing it because that name has no definition. The checker does not claim to reproduce every
Python exception; it reports that this import is unchecked.

The [startup-call lesson](check-python-startup.md) also shows a function replacing
a helper during import. Sharing the check must preserve that unfinished answer.
It cannot turn unchecked writes or unknown calls into checked behavior.

## What this says about cost

The tests compare each shared answer with a separate `fr` analysis of the same
function, using identical sources and rules. They also run Python and compare
`[17, 0]` with the two reported connections. This establishes answer agreement
for these examples. It is not a speed benchmark.

The implementation shares startup preparation, but still indexes the project,
validates inputs, checks each function, and serializes the combined report.
There are no timing or memory measurements here, and the overhead of `fr` and its
SDK compared with executing Python remains unknown. This does not establish
agent savings or support for complete Toolz, Funcy or Pydash imports.
