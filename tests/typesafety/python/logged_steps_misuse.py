# expect: fails
# title: A Logged total spent as a number, rejected by the checker
# misuse-of: logged_steps
from dataclasses import dataclass


@dataclass(frozen=True)
class Logged[T]:
    value: T
    log: tuple[str, ...]


def audited_total(n: int) -> Logged[int]:
    return Logged(n + n // 10, (f"taxed {n}",))


def net(n: int) -> int:
    return audited_total(n) - 45
