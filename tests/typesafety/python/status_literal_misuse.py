# expect: fails
# title: The misspelled status, rejected by the checker
# misuse-of: status_literal
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
