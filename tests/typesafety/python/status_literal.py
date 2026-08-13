# expect: passes
# title: The Status type lists the three valid values
# improves: status_string
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
