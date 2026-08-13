# expect: fails
# title: A Result spent as a number, rejected by the checker
# misuse-of: result_chain
from dataclasses import dataclass


@dataclass(frozen=True)
class Ok[T]:
    value: T


@dataclass(frozen=True)
class Err:
    reason: str


type Result[T] = Ok[T] | Err


def quote(text: str) -> Result[int]:
    return Ok(int(text) * 250) if text.isdigit() else Err(f"not a number: {text!r}")


def total_with_shipping(text: str) -> int:
    return quote(text) + 45
