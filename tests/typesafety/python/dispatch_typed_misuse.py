# expect: fails
# title: A handler that takes the invoice number, rejected by the checker
# misuse-of: dispatch_typed
from collections.abc import Callable
from dataclasses import dataclass
from typing import Literal

type Status = Literal["draft", "sent", "paid"]


@dataclass(frozen=True)
class Invoice:
    number: str
    status: Status


def send(invoice: Invoice) -> Invoice:
    return Invoice(invoice.number, "sent")


def archive(number: str) -> str:
    return f"archived {number}"


HANDLERS: dict[Status, Callable[[Invoice], Invoice]] = {
    "draft": send,
    "sent": archive,
    "paid": send,
}
