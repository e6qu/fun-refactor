# Ask another Python question later

You checked whether one function forwards an input. Later, you want to check another function in the same file.
Save the checked startup preparation between those questions.
Startup preparation identifies functions, captures their default values and checks supported statements that run during import.
In Python, a default value fills an omitted argument: `value=0` uses `0` when the caller leaves out `value`.

This lesson uses separate Python processes for the two questions.
Each question still verifies source files and resolves imports.
Resolving an import means finding the file that supplies a name, such as `relay` from `helpers.py`.

Create an empty folder. Use an installed `fr` and its matching Python SDK, following the [SDK setup](python-contract.md).
The SDK is the Python library that calls `fr` and reads its reports.
Create these files in that folder.

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
`register()` fills that dictionary when Python imports this file.
Its writes meet the [small startup-call rules](check-python-startup.md).

`app.py`:

```python
from helpers import relay, discard


def forwards():
    return sink(relay(source()))


def stopped():
    return sink(discard(source()))
```

`source()` supplies an input. `sink(value)` receives an output.
The following rules tell the checker which names have those roles.
These are assumptions you supply about external functions.

`rules.json`:

```json
{"version":"lesson","sources":["source"],"sinks":["sink"],"propagators":[],"sanitizers":{}}
```

`version` labels this caller-supplied rule set.
Empty `propagators` and `sanitizers` add no external forwarding or cleaning rules.

`check_first.py`:

```python
from pathlib import Path
from fr_ir.context import DirectoryObjectStore
from fr_ir.flow_startup import StartupCache
from fr_ir.runtime import FrClient

client = FrClient(Path.cwd(), executable='fr', timeout=120, max_output_bytes=1_048_576)
store = DirectoryObjectStore(Path('saved-startup'))
cache = StartupCache(store)
handle = client.project('find', 'forwards').definition_target().handle
result = cache.analyze(client, handle, rules=Path('rules.json'), context='generic',
                       steps=4096, depth=8, max_bytes=1_048_576, imports=True)
Path('startup-id.txt').write_text(cache.persist(), encoding='utf-8')
print('Preparation checks:', result.checks)
print('Import checks:', result.import_checks)
print('Finished:', result.answer.report.at('/complete'))
print('Possible input connection:', bool(result.answer.witnesses))
```

`DirectoryObjectStore` saves data in the `saved-startup` directory.
`cache.persist()` returns a content digest: an identifier calculated from the saved bytes.
The second process reads that identifier from `startup-id.txt` and verifies the saved bytes.
Use storage you control. A digest detects changed bytes; it does not prove who performed a check.

`find` returns a function handle, a reference to the function in the current source files.
Every call supplies its settings explicitly.
`steps` limits work tracing values, `depth` limits nested calls, and `max_bytes` limits the combined reply.
`context` selects how caller-supplied cleaning rules apply. This example supplies no cleaning rules.
`imports=True` follows local imports.

`check_later.py`:

```python
from pathlib import Path
from fr_ir.context import DirectoryObjectStore
from fr_ir.flow_startup import StartupCache
from fr_ir.runtime import FrClient

client = FrClient(Path.cwd(), executable='fr', timeout=120, max_output_bytes=1_048_576)
store = DirectoryObjectStore(Path('saved-startup'))
digest = Path('startup-id.txt').read_text(encoding='utf-8')
cache = StartupCache.restore(store, digest)
handle = client.project('find', 'stopped').definition_target().handle
result = cache.analyze(client, handle, rules=Path('rules.json'), context='generic',
                       steps=4096, depth=8, max_bytes=1_048_576, imports=True)
Path('startup-id.txt').write_text(cache.persist(), encoding='utf-8')
print('Preparation checks:', result.checks)
print('Import checks:', result.import_checks)
print('Finished:', result.answer.report.at('/complete'))
print('Possible input connection:', bool(result.answer.witnesses))
```

Run these commands from the example folder:

```sh
python check_first.py
python check_later.py
```

The first command prints:

```text
Preparation checks: 1
Import checks: 1
Finished: True
Possible input connection: True
```

The later command prints:

```text
Preparation checks: 0
Import checks: 1
Finished: True
Possible input connection: False
```

The later question reuses preparation and computes its own answer.
The import check still runs. It includes work checking imported declarations and import order.
Only the separate preparation pass reuses saved work.
An empty connection list answers the question only when `Finished` is `True`, under the [published Python rules](python-contract.md).

## Compare with Python itself

Create `run_example.py`:

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

A `lambda` creates a small function from one expression.
`builtins` holds names Python can find without an import in each file.
These definitions supply the example's input and output functions.
Run `python run_example.py`. It prints `ready`, `17` and `0`, each on its own line.
The checker inspects source code without running this script.

## Change the inputs and ask again

Change `discard` in `helpers.py` to return `value`. Run `python check_later.py` again.
It now prints one preparation check, one import check, a finished answer and a possible input connection.
The saved source identity differs, so the checker prepares the changed file again.
`python run_example.py` now prints `ready`, `17` and `17`.

Add `unknown()` after `register()` in `helpers.py`. Run `python check_later.py` again.
The answer is unfinished because that startup call has unknown effects.
The checker saves no preparation from an unfinished answer.
Python raises `NameError` during import because `unknown` has no definition.
An unfinished answer with no connection establishes nothing about output safety.
Remove `unknown()` and run `python check_later.py` to check the repaired example.

Source edits, configuration changes and competing import files invalidate saved preparation.
For example, adding `helpers.pyi` changes import admission even when `helpers.py` stays unchanged.
A `.pyi` file describes Python declarations for checking; this model refuses competing source and declaration files.
Every request rechecks those choices, including files that were previously absent.

## Work, limits and stored data

One cache keeps preparation for one defining file and its local imports.
Asking about another file replaces that entry after a finished check.
Only checked preparation from a finished answer enters storage.
Corrupt records fail visibly. The reader accepts one strict current format.

Rules, context and analysis limits apply afresh to every question.
They do not change the stored startup declarations, so changing them alone can still reuse preparation.
The function answer always uses the settings supplied for that request.

Imports retain their limits: 16 modules, 256 KiB of source and 128 import searches.
A KiB is 1,024 bytes. Each supported initializer retains its 128-node limit.
The new answer and saved record share the requested output limit, up to 1 MiB.
One MiB is 1,048,576 bytes.
The command refuses a combined reply that will not fit.
Reusing preparation grants no extra analysis steps or larger source allowance.

This lesson compares behavior with ordinary Python and checks against fresh answers.
It does not measure elapsed time or claim a speedup.
File checks, import resolution, parsing, process startup and storage costs still apply.
Their total overhead compared with a fresh request remains unmeasured.

The command-line mode is `fr project dataflow HANDLE --summaries --with-startup` with your explicit analysis settings.
It returns an answer, preparation counts and a saved startup record when eligible.
Pass that record with `--reuse-startup FILE --startup-digest DIGEST` on a later request.
`DIGEST` is SHA-256 of the record's compact JSON with sorted keys and unescaped Unicode.
The Python API handles that encoding and storage.
