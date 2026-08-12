# expect: fails
# title: A variable typed str, rejected at the mode parameter
# misuse-of: literal_flag
"""A value typed `str` could be anything, so the checker refuses to pass it
where only two values are allowed. Type the variable as the literal, or pass
the value directly."""

from typing import Literal


def read_log(path: str, mode: Literal["text", "binary"]) -> int:
    record_size = 1 if mode == "text" else 8
    return len(path) * record_size


def tail(chosen: str) -> int:
    return read_log("app.log", chosen)  # error: str is wider than the two values
