# expect: passes
# run: yes
# title: Logged pairs each result with the log that produced it
# improves: printed_steps
"""`Logged` pairs a value with the log that produced it. `and_then` runs the
next step and concatenates the trails, so the log arrives with the answer,
as data. This shape is usually called the Writer monad."""

from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Logged[T]:
    value: T
    log: tuple[str, ...]


def and_then[T, U](logged: Logged[T], step: Callable[[T], Logged[U]]) -> Logged[U]:
    result = step(logged.value)
    return Logged(result.value, logged.log + result.log)


def double(n: int) -> Logged[int]:
    return Logged(n * 2, (f"doubled {n}",))


def add_tax(n: int) -> Logged[int]:
    return Logged(n + n // 10, (f"taxed {n}",))


def total(n: int) -> Logged[int]:
    return and_then(and_then(Logged(n, ()), double), add_tax)


assert total(100).value == 220
assert total(100).log == ("doubled 100", "taxed 200")
