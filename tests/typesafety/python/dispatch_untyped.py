# expect: passes
# title: A table of handlers typed Any takes a handler that cannot run
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class Invoice:
    number: str
    status: str


def send(invoice: Invoice) -> Invoice:
    return Invoice(invoice.number, "sent")


def archive(number: str) -> str:
    return f"archived {number}"


HANDLERS: dict[str, Callable[..., Any]] = {
    "draft": send,
    "sent": archive,
}


def advance(invoice: Invoice) -> Any:
    return HANDLERS[invoice.status](invoice)
