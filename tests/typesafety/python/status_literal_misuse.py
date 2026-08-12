# expect: fails
# title: A typo is now a type error
"""The same `advance`, called with a misspelled status."""

from typing import Literal

type Status = Literal["draft", "sent", "paid"]


def advance(status: Status) -> Status:
    match status:
        case "draft":
            return "sent"
        case "sent":
            return "paid"
        case "paid":
            return "paid"


def submit() -> Status:
    return advance("snet")  # error: not one of the three statuses
