# expect: fails
# title: A carrier whose quote returns text, rejected by the checker
# misuse-of: carrier_protocol
from typing import Protocol


class Carrier(Protocol):
    def quote_pence(self, kilograms: float) -> int: ...


class ChattyCourier:
    def quote_pence(self, kilograms: float) -> str:
        return f"about {int(kilograms) * 100} pence"


def cheapest(carriers: list[Carrier], kilograms: float) -> int:
    return min(carrier.quote_pence(kilograms) for carrier in carriers)


best = cheapest([ChattyCourier()], 9.5)
