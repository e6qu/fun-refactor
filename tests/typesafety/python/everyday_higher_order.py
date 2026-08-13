# expect: passes
# run: yes
# title: The @timed decorator adds the measurement once
# improves: plumbing_by_hand
import time
from collections.abc import Callable

durations: list[float] = []


def timed[**P, R](operation: Callable[P, R]) -> Callable[P, R]:
    def wrapped(*args: P.args, **kwargs: P.kwargs) -> R:
        started = time.perf_counter()
        try:
            return operation(*args, **kwargs)
        finally:
            durations.append(time.perf_counter() - started)
    return wrapped


@timed
def area(width: int, height: int) -> int:
    return width * height


@timed
def greet(name: str) -> str:
    return f"hello {name}"


assert area(3, 4) == 12
assert greet("ada") == "hello ada"
assert len(durations) == 2
