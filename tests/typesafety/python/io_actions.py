# expect: passes
# run: yes
# title: IO describes the fetch, retry wraps it, and run performs it
# improves: inline_retry
from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class IO[T]:
    run: Callable[[], T]


def of[T](value: T) -> IO[T]:
    return IO(lambda: value)


def and_then[T, U](action: IO[T], step: Callable[[T], IO[U]]) -> IO[U]:
    return IO(lambda: step(action.run()).run())


def retry[T](times: int, action: IO[T]) -> IO[T]:
    def attempt() -> T:
        failures = 0
        while True:
            try:
                return action.run()
            except ConnectionError:
                failures += 1
                if failures >= times:
                    raise
    return IO(attempt)


# A connection that fails twice and then answers, so the retry is observable.
_calls = {"count": 0}


def flaky_fetch() -> str:
    _calls["count"] += 1
    if _calls["count"] < 3:
        raise ConnectionError("try again")
    return "payload"


greeting = and_then(retry(3, IO(flaky_fetch)), lambda text: of(text.upper()))

assert _calls["count"] == 0  # nothing has run yet
assert greeting.run() == "PAYLOAD"
assert _calls["count"] == 3  # two failures, one answer
