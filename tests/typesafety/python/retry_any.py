# expect: passes
# title: After retry, the checker forgets the signature of fetch
"""`retry` returns `Callable[..., Any]`, so the checker no longer sees the
parameters or the result of the function inside it."""

from collections.abc import Callable
from typing import Any


def retry(times: int, operation: Callable[..., Any]) -> Callable[..., Any]:
    def attempt(*args: Any, **kwargs: Any) -> Any:
        failures = 0
        while True:
            try:
                return operation(*args, **kwargs)
            except ConnectionError:
                failures += 1
                if failures >= times:
                    raise
    return attempt


def fetch(url: str, timeout: int) -> str:
    return f"GET {url} within {timeout}s"


patient_fetch = retry(3, fetch)
# The checker accepts both of these. The second fails at run time.
fine = patient_fetch("https://example.test", 10)
wrong = patient_fetch(10, "https://example.test")
