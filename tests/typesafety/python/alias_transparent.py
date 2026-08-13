# expect: passes
# title: An alias is only a name: minutes still pass as Seconds
type Seconds = int


def wait_before_retry(delay: Seconds) -> str:
    return f"sleeping {delay}s"


def plan() -> str:
    minutes = 5
    return wait_before_retry(minutes)  # accepted: Seconds and int are the same type
