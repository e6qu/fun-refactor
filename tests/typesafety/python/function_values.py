# expect: passes
# run: yes
# title: The key's full type says what the sort will hand it
# improves: function_any
from collections.abc import Callable
from dataclasses import dataclass
from typing import NewType

Pence = NewType("Pence", int)


@dataclass(frozen=True)
class InvoiceLine:
    item: str
    pence: Pence
    quantity: int


def picking_list(
    lines: list[InvoiceLine], key: Callable[[InvoiceLine], int]
) -> list[InvoiceLine]:
    return sorted(lines, key=key)


basket = [
    InvoiceLine("saddle", Pence(155), 1),
    InvoiceLine("bell", Pence(80), 3),
]

cheapest_first = picking_list(basket, lambda line: line.pence)
fewest_first = picking_list(basket, lambda line: line.quantity)

assert [line.item for line in cheapest_first] == ["bell", "saddle"]
assert [line.item for line in fewest_first] == ["saddle", "bell"]
