# expect: passes
# title: One alias names the callback shape, and one constant names the number
# improves: alias_repeated
"""An alias earns its keep on a compound type: the name reads, and one edit
changes every signature. A `Final` constant does the same for a magic number."""

from collections.abc import Callable
from typing import Final

type Milliseconds = int
type RetryPolicy = Callable[[int, Exception], Milliseconds]

DEFAULT_BACKOFF: Final[Milliseconds] = 30_000


def fixed_backoff(attempt: int, error: Exception) -> Milliseconds:
    return DEFAULT_BACKOFF


def doubling_backoff(attempt: int, error: Exception) -> Milliseconds:
    return DEFAULT_BACKOFF * (1 << attempt)


def run_with_retries(policy: RetryPolicy) -> Milliseconds:
    return policy(1, ValueError("transient"))
