# expect: passes
# title: A wrapper that keeps the signature
"""`retry` takes a function and returns one with the same parameters and the
same result. The type parameters carry the whole signature through."""

from collections.abc import Callable


def retry[**P, R](times: int, operation: Callable[P, R]) -> Callable[P, R]:
    def attempt(*args: P.args, **kwargs: P.kwargs) -> R:
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
result: str = patient_fetch("https://example.test", 10)
