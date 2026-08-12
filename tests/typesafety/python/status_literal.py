# expect: passes
# title: A closed set of strings
"""`Literal` lists every value the type allows. The checker rejects the rest."""

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
