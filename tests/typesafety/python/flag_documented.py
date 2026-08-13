# expect: passes
# title: The docstring says text or binary, and the checker cannot read it
def read_log(path: str, mode: str) -> int:
    """mode is "text" or "binary"."""
    record_size = 1 if mode == "text" else 8
    return len(path) * record_size


def tail() -> int:
    return read_log("app.log", "binry")
