# expect: passes
# title: A literal type lists every status that exists
# improves: status_string
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
