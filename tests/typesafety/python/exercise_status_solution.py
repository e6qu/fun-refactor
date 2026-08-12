# expect: passes
# title: Status as a literal type catches the typo
# improves: exercise_status_start
"""A closed set and an exhaustive match. The typo from the start is now a
compile error, and so is a forgotten status."""

from typing import Literal, assert_never

type Status = Literal["received", "picked", "shipped"]


def next_action(status: Status) -> str:
    match status:
        case "received":
            return "start picking"
        case "picked":
            return "pack the box"
        case "shipped":
            return "send the tracking mail"
        case _:
            assert_never(status)
