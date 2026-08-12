# expect: passes
# title: A parameter that takes two values, said in the type
"""The docstring used to say "mode is 'text' or 'binary'". `Literal` moves that
sentence into the signature, where the checker reads it."""

from typing import Literal


def read_log(path: str, mode: Literal["text", "binary"]) -> int:
    record_size = 1 if mode == "text" else 8
    return len(path) * record_size


def tail() -> int:
    return read_log("app.log", "binary")
