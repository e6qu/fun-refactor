# expect: passes
# title: The mode parameter accepts only text or binary
# improves: flag_documented
from typing import Literal


def read_log(path: str, mode: Literal["text", "binary"]) -> int:
    record_size = 1 if mode == "text" else 8
    return len(path) * record_size


def tail() -> int:
    return read_log("app.log", "binary")
