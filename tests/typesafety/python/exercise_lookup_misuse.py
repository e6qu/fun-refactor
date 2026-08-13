# expect: fails
# title: A Result where greet needs the plain user id, rejected by the checker
# misuse-of: exercise_lookup_solution
from dataclasses import dataclass


@dataclass(frozen=True)
class Ok[T]:
    value: T


@dataclass(frozen=True)
class Err:
    reason: str


type Result[T] = Ok[T] | Err


def find_user_id(login: str) -> Result[str]:
    return Ok("u7") if login == "ada" else Err(f"no user for login {login!r}")


def greet(user_id: str) -> str:
    return f"welcome {user_id}"


banner = greet(find_user_id("ada"))
