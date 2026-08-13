# expect: passes
# title: RetryPolicy and DEFAULT_BACKOFF give the type and the number names
# improves: alias_repeated
from collections.abc import Callable
from typing import Final

type Milliseconds = int
type RetryPolicy = Callable[[int, Exception], Milliseconds]

DEFAULT_BACKOFF: Final[Milliseconds] = 30_000


def fixed_backoff(_attempt: int, _error: Exception) -> Milliseconds:
    return DEFAULT_BACKOFF


def doubling_backoff(attempt: int, _error: Exception) -> Milliseconds:
    return DEFAULT_BACKOFF * (1 << attempt)


def run_with_retries(policy: RetryPolicy) -> Milliseconds:
    return policy(1, ValueError("transient"))
