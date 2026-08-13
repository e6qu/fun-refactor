# expect: passes
# title: The retry loop lives inside the business function
def fetch_greeting(attempts_left: int = 3) -> str:
    failures = 0
    while True:
        try:
            return unreliable_fetch().upper()
        except ConnectionError:
            failures += 1
            if failures >= attempts_left:
                raise


def unreliable_fetch() -> str:
    raise ConnectionError("try again")
