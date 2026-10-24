# Check a translation and understand a value trace

Suppose a service has 11 items and puts five on each page. It needs three pages.
A helper moved from Rust to Python must still return three, reject an invalid page size, and handle large counts without rounding through a floating-point number.
Separately, a value trace must tell you when a Python exception or a change to a shared list lies outside its model.

These are behavior checks, not speed comparisons. They do not measure agent savings or language overhead.

## Translate a page-count calculation

A behavior contract states the inputs a function accepts, the result it promises and how it handles errors.
We committed the [chosen contract](../tests/agent-eval/checked-translation/contract.json) before collecting results or changing translation support.
Both inputs are signed 64-bit integers: whole numbers from -9,223,372,036,854,775,808 through 9,223,372,036,854,775,807.
The Python inputs are ordinary `int` values in that range. Objects with custom arithmetic and Python's `bool` type are outside this contract.
The quotient counts full pages. The remainder counts items left after those pages.

The function promises:

- A negative item count, or a page size of zero or less, returns `-1` before division.
- Otherwise, return enough whole pages for every item, with no unused extra page.
- Calculate the quotient and remainder, then add a page only when something remains.

For example, 10 items at five per page need two pages; 11 need three; zero items need none.
`page_count(11, 0)` returns `-1` without dividing by zero.
The starting [Rust](../tests/agent-eval/checked-translation/pages.rs) and [Python](../tests/agent-eval/checked-translation/pages.py) files contain the complete function, with no external dependencies.

Copy the Rust file to a scratch directory. Preview the translation first:

```sh
fr --json -C /path/to/scratch translate pages.rs python --out pages.py
```

The preview describes the proposed edit and leaves `pages.py` unwritten.
After reviewing it, run the same command with `--write` to create the file.
For the other direction, start with the Python file in another scratch directory and name `rust` as the target.
A signature describes a function's name, named inputs and returned value type.
The translation report counts supported signatures and anything carried into the output without a translation.
A clean report alone does not check the generated program's behavior.

The independent check runs every combination of item counts -2 through 255 and page sizes -2 through 32: 9,030 pairs.
It adds eleven boundary pairs, including the largest and smallest signed 64-bit inputs.
It repeats each direction after renaming the function and moving its file. Translation rules must not depend on this function's name or location.
Every source and generated function is compared with a separate mathematical calculation.
For example, the largest signed 64-bit count with page size two must return 4,611,686,018,427,387,904.
A conversion through an approximate floating-point value can get such boundaries wrong.

Rust can overflow when an integer calculation exceeds its fixed range; Python integers grow as needed.
The test enables Rust's overflow checks explicitly.
The function avoids the tempting expression `items + size - 1`, which can overflow before division.
Its actual quotient and possible increment remain in range under the contract.
The large-input check caught a real translator error: converting a Rust integer quotient through Python's floating-point division rounded the result.
The translator now keeps that division in integer arithmetic and evaluates each operand once in source order.
See the [Rust operator rules](https://doc.rust-lang.org/reference/expressions/operator-expr.html) and [Python arithmetic rules](https://docs.python.org/3.14/reference/expressions.html#binary-arithmetic-operations).
The finite tests and this arithmetic argument do not prove arbitrary translations correct.

## Read the limits of a Python value trace

A scalar value trace follows simple values, such as numbers, through selected operations and helper calls.
A witness describes one possible value connection in that model. It is not a recorded execution.
The result's `complete` field says whether that declared model finished its work.
It does not say that Python will run every shown operation successfully.

For example, a function might read customer input, divide by zero, then pass the input to `send`.
Ordinary Python raises `ZeroDivisionError` at the division and never calls `send`.
The scalar model does not simulate that implicit exception: an error caused by an operation without an explicit `raise` statement.
Its `effect_limits` list now identifies the division's exact file and source range.
Its `runtime_effects.exception_safety_established` field is false, including when the scalar report is complete.
Do not turn that possible value connection into a claim that the send will execute.

An explicit supported `raise` leaves through `finally` cleanup, which can replace the pending return or raise.
The [cleanup repair lesson](check-python-cleanup.md) explains that behavior with a customer-message bug and a checked repair.
Exception handlers using `try` and `except` can match an error and resume execution elsewhere.
Supported explicit builtin errors now follow matching clauses in order, including through `finally` cleanup.
The [error-handler repair](catch-python-errors.md) changes an application that catches `TypeError` when Boltons raises `ValueError`.
Implicit errors, custom classes and bound error objects still exceed this model.

Two names can also refer to the same mutable object, meaning an object whose contents can change:

```python
values = [0]
alias = values
alias[0] = 17
print(values[0])  # Prints 17: both names refer to the same list.
```

The scalar model does not track shared list identities or writes through another name.
It keeps such answers incomplete and identifies the item write, item read or unchecked method call.
An empty witness list in an incomplete answer cannot show that customer input never reaches an output.
Python SDK users can inspect `analysis.runtime_effects.sites`, including each site's kind, explanation and source occurrence.
The SDK refuses missing disclosures, altered safety claims and locations omitted by a response budget.
Saved-answer reuse also refuses a record that turns the scalar model into an exception-safety claim.

The independent examples exercise explicit raises, division errors, handlers, `finally`, list writes through another name and list methods.
Each runs with inputs 17 and 31, then runs again after renaming the input/output operations and moving the source file.
The checker compares ordinary Python executions with expectations written before collection.
The division example deliberately retains the difference between Python and the scalar model.
No exception engine or shared-object analysis is claimed by these explanations.

## Follow customer text through string operations

Suppose a service trims spaces from a customer label and changes it to lowercase before sending it.
Changing the spelling keeps the customer's value connected to the output.
For example, `<SCRIPT>` becomes `<script>` and still needs the service's usual HTML escaping.

Create `subject.py` in a scratch project:

```python
def read_user():
    return '<SCRIPT>'

def send(value):
    return value

def entry():
    label = str(read_user())
    return send(label.strip().lower())
```

`str` converts a value to text. `strip` removes spaces at either end; `lower` changes letter case.
Each returns a new string. The receiver is the value before the method name: `label` in `label.strip()`.
The example supplies small stand-ins for the service's input and output functions.
Create `rules.json` to name them for this question:

```json
{"version":"text-flow","sources":["read_user"],"sinks":["send"],"sanitizers":{},"propagators":[]}
```

A source supplies the value to follow. A sink receives it.
The empty sanitizer map declares no operation that makes the value safe for HTML.
Find the entry function:

```sh
fr --json -C /path/to/scratch project find entry --in subject.py
```

Copy its handle, the identifier for that function in this source revision, into the following command:

```sh
fr --json -C /path/to/scratch project dataflow HANDLE \
  --imports --summaries --standard-library builtins \
  --rules /path/to/scratch/rules.json --context html \
  --steps 4096 --depth 8 --bytes 262144
```

`--imports` includes imported Python files. `--summaries` checks helper return behavior and reuses it within the question.
`--standard-library builtins` explicitly selects the CPython builtins model, including `str` and the checked string methods.
The three limits bound analysis steps, helper-call depth and report bytes.
The answer should finish and retain a possible connection from `read_user` to `send`.

Now insert `label = 'PUBLIC'` before the return.
The answer should finish without that connection: reassignment replaced the customer value.
Instead assigning an arbitrary object keeps the answer incomplete, even if that object has a method called `lower`.
That method can return a number or run unrelated code.

The model accepts plain strings from literals, checked conversions, local assignments and helpers whose modeled normal returns all produce strings.
A helper that sometimes returns text and sometimes returns a number cannot support the same conclusion.
At a branch or loop join, every arriving path must establish the string fact.
Type annotations alone do not establish the runtime value.

The checked methods are `lower`, `upper`, `casefold`, `capitalize`, `swapcase`, `title`, `strip`, `lstrip`, `rstrip`, `removeprefix`, `removesuffix` and `replace`.
Arguments must be positional text values. Stripping also accepts `None`; a replacement count must be a literal integer.
Unknown receivers, expanded arguments and unsupported signatures leave the answer incomplete.
Possible origins from both the receiver and arguments remain visible.
This model follows value connections. Input that only chooses between branches needs a separate information-flow model.
The runtime exception and shared-object limits explained above still apply.

The [authored examples](../tests/agent-eval/python-text-flow/cases.json) state Python outputs and expected analysis results before collection.
They include Unicode, reassignment, mixed return types, recursion and unsupported calls, then repeat after renaming and moving.
These are correctness checks. They measure neither speed nor language overhead.

## Repeat the checks

Use the hosted `language-behavior` group in the [evidence workflow](../.github/workflows/refinement-evidence.yml).
It builds `fr`, runs the independent controls and retains source, generated code, answers and failures.
Full collections run on GitHub to protect the shared workstation.

```sh
gh workflow run refinement-evidence.yml --ref YOUR_BRANCH \
  -f group=language-behavior -f prefix=YOUR_COLLECTION_NAME
```

`YOUR_BRANCH` selects the source revision; `YOUR_COLLECTION_NAME` labels the requested collection.
The collector also uses `fr project find` to inspect each selected function before translating or tracing it.
The native test target `language_behavior` repeats both checks in CI.
Retained-result audits reject changed inputs, altered outputs and missing cases. A new fingerprint on a wrong answer does not make it correct.
