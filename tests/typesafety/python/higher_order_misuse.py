# expect: fails
# title: The wrapped fetch still rejects swapped arguments
# misuse-of: higher_order
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
result = patient_fetch(10, "https://example.test")
