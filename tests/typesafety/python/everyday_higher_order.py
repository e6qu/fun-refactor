# expect: passes
# run: yes
# title: filter and map take the test and the extractor as values
# improves: plumbing_by_hand
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


def amount(line: InvoiceLine) -> Pence:
    return line.pence


def gift_total(lines: list[InvoiceLine]) -> Pence:
    return Pence(sum(map(amount, filter(is_gift, lines))))


basket = [
    InvoiceLine("saddle", Pence(155), gift=True),
    InvoiceLine("spokes", Pence(36), gift=False),
    InvoiceLine("bell", Pence(80), gift=True),
]
assert gift_total(basket) == 235
