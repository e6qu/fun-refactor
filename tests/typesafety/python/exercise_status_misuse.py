# expect: fails
# title: The misspelled status, rejected by the checker
# misuse-of: exercise_status_solution
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


def handle() -> str:
    return next_action("recieved")
