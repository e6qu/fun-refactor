# expect: passes
# title: An alias names the intent and enforces nothing
"""`type Seconds = int` documents the parameter. It is still `int` to the checker."""

type Seconds = int


def wait_before_retry(delay: Seconds) -> str:
    return f"sleeping {delay}s"


def plan() -> str:
    minutes = 5
    # The checker accepts this call. The alias and int are the same type,
    # so nothing points out that these are minutes.
    return wait_before_retry(minutes)
