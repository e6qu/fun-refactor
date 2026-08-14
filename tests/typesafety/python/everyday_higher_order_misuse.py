# expect: fails
# title: An extractor that returns the item name, rejected by the checker
# misuse-of: everyday_higher_order
from dataclasses import dataclass
from typing import NewType

Pence = NewType("Pence", int)


@dataclass(frozen=True)
class InvoiceLine:
    item: str
    pence: Pence
    gift: bool


def is_gift(line: InvoiceLine) -> bool:
    return line.gift


def label(line: InvoiceLine) -> str:
    return line.item


def gift_total(lines: list[InvoiceLine]) -> Pence:
    return Pence(sum(map(label, filter(is_gift, lines))))
