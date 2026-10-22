# Make your first checked change

Practice renaming `greet` to `welcome` while preserving the greeting. Use a new empty folder, an installed `fr` and Python 3.
Create these files before running commands. `def` defines a function; `assert` checks its expected result.

Save as `app.py`:
```python
def greet(name):
    return "hello " + name

def run():
    return greet("Ada")
```

Save as `test_app.py`:
```python
from app import greet, run

assert greet("Ada") == "hello Ada"
assert run() == "hello Ada"
```

Create `.fr/checks.json`. A check names the command that tests the behavior; listing it does not execute it.
```json
{
  "schema": 1,
  "checks": [{
    "name": "greeting",
    "argv": ["python3", "-B", "test_app.py"],
    "cwd": ".",
    "timeout_seconds": 10,
    "covers": ["greeting and its caller"]
  }]
}
```

Find the function and review a rename:
```sh
fr project find greet --signature --source --bytes 512
fr refs greet
fr rename greet welcome
fr rename greet welcome --save-plan
```
The preview prints a diff: `-` marks an old line and `+` its replacement.
Check that the declaration, caller and test import change, while the greeting stays the same.
Saving records the proposed edit without applying it. Replace `<FIRST_TX>` with its returned `transaction` number, including the brackets.

```sh
fr history show '<FIRST_TX>'
fr history apply '<FIRST_TX>' --write
fr checks
```
An applied transaction is a saved edit that changed the files. The check listing returns a `basis`, identifying the inputs you reviewed.
Copy that value into `<FIRST_CHECK_BASIS>`:

```sh
fr checks --run greeting --basis '<FIRST_CHECK_BASIS>' --record-for '<FIRST_TX>'
```
Require `passed: true`. Syntax acceptance alone does not check the greeting.
If an affected file changes before application, prepare a fresh preview rather than forcing the saved edit through.

Preview and apply the reversal:
```sh
fr history undo '<FIRST_TX>'
fr history undo '<FIRST_TX>' --write
fr checks
```
Undo restores the original function and callers. The source changed, so copy the new check listing's basis into `<UNDO_CHECK_BASIS>`:
```sh
fr checks --run greeting --basis '<UNDO_CHECK_BASIS>'
```
Require another passing result. These tests check this example, not every behavior in a larger project.
Continue with [structural changes](change.md), [implementation edits](author.md) or [patch delivery](git.md) as needed.
