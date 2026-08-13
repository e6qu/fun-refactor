# expect: fails
# title: An IO description where its result is required, rejected by the checker
# misuse-of: io_actions
from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class IO[T]:
    run: Callable[[], T]


def fetch_greeting() -> IO[str]:
    return IO(lambda: "payload")


def send(payload: str) -> str:
    return f"sent {payload}"


delivery = send(fetch_greeting())
