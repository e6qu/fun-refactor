# expect: passes
# run: yes
# title: The table declares one handler shape, and every entry is held to it
# improves: dispatch_untyped
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


def record_payment(invoice: Invoice) -> Invoice:
    return Invoice(invoice.number, "paid")


def keep(invoice: Invoice) -> Invoice:
    return invoice


HANDLERS: dict[Status, Callable[[Invoice], Invoice]] = {
    "draft": send,
    "sent": record_payment,
    "paid": keep,
}


def advance(invoice: Invoice) -> Invoice:
    return HANDLERS[invoice.status](invoice)


assert advance(Invoice("INV-7", "draft")).status == "sent"
assert advance(Invoice("INV-7", "paid")).status == "paid"
