# expect: passes
# run: yes
# title: quote returns Ok with a price, or Err with the reason
# improves: nullable_chain
"""The same three steps. `Result` holds a value or a reason, and `and_then`
chains the steps and stops at the first failure."""

from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Ok[T]:
    value: T


@dataclass(frozen=True)
class Err:
    reason: str


type Result[T] = Ok[T] | Err


def and_then[T, U](result: Result[T], step: Callable[[T], Result[U]]) -> Result[U]:
    match result:
        case Ok(value):
            return step(value)
        case Err():
            return result


def parse_quantity(text: str) -> Result[int]:
    return Ok(int(text)) if text.isdigit() else Err(f"not a number: {text!r}")


def check_stock(quantity: int) -> Result[int]:
    return Ok(quantity) if quantity <= 10 else Err("only 10 in stock")


def quote(text: str) -> Result[int]:
    return and_then(and_then(parse_quantity(text), check_stock), lambda q: Ok(q * 250))


assert quote("3") == Ok(750)
assert quote("99") == Err("only 10 in stock")
assert quote("many") == Err("not a number: 'many'")
