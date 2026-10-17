# Why is this Python trace unfinished?

You want to know whether a submitted value reaches an output through an imported helper.
Here, a value trace describes where supplied data may pass through supported code.
The trace finds a possible connection, but says it could not finish checking.
This guide shows how to explain that answer before changing code or increasing any limit.

An import can run code before making a helper available.
A module is a Python source file. A package groups modules and can run its `__init__.py` file during import.
The analyzer may find the helper while leaving that earlier code unchecked.
It can also find a source file without having enough checking budget to analyze it.
Neither situation establishes that the imported name is missing at runtime.

## Create a complete example

You need Python 3, an installed `fr` executable on your command path and its matching Python SDK.
Follow the [installation guide](../README.md#install) and [Python client setup](../README.md#give-fr-to-an-agent) first.
Use a new directory for the following files and run the commands there.

Save this as `helpers.py`. Calling `start()` while Python imports the file creates the metadata value.
The analyzer does not yet check that import-time call, even though this particular call returns zero.

```python
def start():
    return 0

metadata = start()

def relay(value):
    return value
```

Save this as `app.py`. `read_input` supplies the chosen input and `send` represents the output.
They are ordinary functions here, so the example also runs in Python.

```python
from helpers import relay

def read_input():
    return 17

def send(value):
    return value

def entry():
    return send(relay(read_input()))
```

Save this as `rules.json`. It tells the analyzer which function supplies input and which receives output.
The rule label identifies this chosen configuration; it does not change Python's behavior.

```json
{
  "version": "import-example",
  "sources": ["read_input"],
  "sinks": ["send"],
  "sanitizers": {},
  "propagators": []
}
```

## Analyze and explain the answer

The client below sends commands to the installed executable.
It finds the selected function and receives a handle: an identifier for that function in the current source.
The cache keeps the answer in memory; this example does not create a persistent cache.
`summaries=True` follows supported helper calls, and `imports=True` includes supported local imports.
`steps` limits the amount of analysis work.
The first `max_bytes` caps the original structured answer; the second caps the explanation text.
These separate text budgets do not change the limits on source files, imports or elapsed execution time.

Save this as `explain.py`:

```python
import json
from pathlib import Path

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_import_explanation import ImportExplanation
from fr_ir.runtime import FrClient

client = FrClient(".")
target = client.project("find", "entry", "--in", "app.py").definition_target().handle
analysis = FlowCache(MemoryObjectStore()).analyze(
    client, target, rules=Path("rules.json"), summaries=True, imports=True,
    steps=4096, max_bytes=1048576,
)
Path("answer.json").write_text(json.dumps(analysis.report.to_data()))
explanation = ImportExplanation.from_report(analysis.report)
print(explanation.render(max_bytes=2048), end="")
```

Run `python3 explain.py`. The explanation says the recorded import check is unfinished.
It counts one unchecked import-time statement and points to line 4 of `helpers.py`.
The overall value trace remains unfinished even if it contains a possible input-to-output connection.
This does not mean the program crashes: `python3 -c 'import app; print(app.entry())'` prints 17.

For this example, replace `metadata = start()` with `metadata = 0`, then run `python3 explain.py` again.
The new analysis can finish under its stated rules.
That result concerns the declared value-flow model; it does not prove every aspect of Python execution.

## Explain a report you already saved

You can explain `answer.json` without reopening the original project:

```sh
python3 -m fr_ir.flow_import_explanation answer.json --max-bytes 2048 --max-items 8
```

The input must be a current import-enabled report, including its required dependency records.
Missing fields, inconsistent counts, duplicate JSON fields and obsolete forms refuse rather than receive defaults.
The saved file must fit within 1 MiB, meaning 1,048,576 bytes.
The command does not rerun the analysis or check whether source files have changed.
For a fresh answer after an edit, run the analysis again before explaining it.

The output budget counts UTF-8 bytes, the encoding used to store this text.
Whole details that do not fit remain omitted; the final lines state how many groups and file summaries appear.
They also distinguish source locations omitted by the original analyzer from the smaller sample shown in the explanation.
The structured `gaps`, `conditions` and `initialization` fields keep the recorded details available to a Python caller.
One import group can contain several requested names sharing the same importing file, provider and unresolved condition.
An import request names one wanted value or represents an import using `*`, also called a wildcard import.
The group counts refer to those recorded requests, rather than distinct lines of Python.

## Read the real-package example carefully

The retained [Pydash capture](../tests/agent-eval/python-symbolic/frozen-run.json) read eight source files,
reached its existing 256 KiB source limit and left 420 import-time statements unchecked.
KiB means 1,024 bytes. Those unchecked statements include calls, decorators and writes performed while importing.
The new explanation separates selected source files that were not analyzed from bindings the analyzer could not resolve in loaded files.
It does not turn either condition into a claim that Python lacks that name.
It also keeps missing local source separate from installed or standard-library modules outside the checked project root.

This view explains an existing answer; it adds no Python language rules and leaves package tracing unfinished.
Its checking and grouping take additional work, including reading and validating the original report.
That CPU and memory overhead has not been measured here.
A shorter explanation alone does not establish faster analysis or a cheaper agent task.
