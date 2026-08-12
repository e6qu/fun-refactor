# expect: passes
# title: The docstring carries a rule the checker never sees
"""The comment says two values are allowed. Nothing enforces it, so any string
arrives here."""


def read_log(path: str, mode: str) -> int:
    """mode is "text" or "binary"."""
    record_size = 1 if mode == "text" else 8
    return len(path) * record_size


def tail() -> int:
    return read_log("app.log", "binry")  # typo: silently reads as binary
