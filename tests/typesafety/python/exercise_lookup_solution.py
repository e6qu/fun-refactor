# expect: passes
# run: yes
# title: Each lookup returns a Result with its own reason
# improves: exercise_lookup_start
"""The same three steps, with a Result. Each failure carries its reason, and
`and_then` threads the chain."""

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


def find_user_id(login: str) -> Result[str]:
    return Ok("u7") if login == "ada" else Err(f"no user for login {login!r}")


def find_cart(user_id: str) -> Result[list[str]]:
    return Ok(["book"]) if user_id == "u7" else Err(f"no cart for {user_id}")


def head(items: list[str]) -> Result[str]:
    return Ok(items[0]) if items else Err("the cart is empty")


def first_item(login: str) -> Result[str]:
    return and_then(and_then(find_user_id(login), find_cart), head)


assert first_item("ada") == Ok("book")
assert first_item("bob") == Err("no user for login 'bob'")
