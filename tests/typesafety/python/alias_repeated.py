# expect: passes
# title: The same function type is written out three times
"""The same function type is spelled out three times, and the magic number sits
bare where a name should be."""

from collections.abc import Callable


def fixed_backoff(attempt: int, error: Exception) -> int:
    return 30_000


def doubling_backoff(attempt: int, error: Exception) -> int:
    return 30_000 * (1 << attempt)


def run_with_retries(policy: Callable[[int, Exception], int]) -> int:
    return policy(1, ValueError("transient"))
