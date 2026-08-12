# expect: passes
# title: The timing lines repeat in every function
"""Two functions want the same measurement, so the same three lines appear in
both. The next function that needs timing copies them again."""

import time

durations: list[float] = []


def area(width: int, height: int) -> int:
    started = time.perf_counter()
    result = width * height
    durations.append(time.perf_counter() - started)
    return result


def greet(name: str) -> str:
    started = time.perf_counter()  # the same three lines, again
    result = f"hello {name}"
    durations.append(time.perf_counter() - started)
    return result
