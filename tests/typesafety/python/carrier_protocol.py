# expect: passes
# run: yes
# title: A Protocol accepts any carrier with the right method, no inheritance asked
# improves: carrier_inheritance
from typing import Protocol


class Carrier(Protocol):
    def quote_pence(self, kilograms: float) -> int: ...


class RoyalPost:
    def quote_pence(self, kilograms: float) -> int:
        return int(kilograms * 120)


class VillageCourier:
    def quote_pence(self, kilograms: float) -> int:
        return 90


def cheapest(carriers: list[Carrier], kilograms: float) -> int:
    return min(carrier.quote_pence(kilograms) for carrier in carriers)


assert cheapest([RoyalPost(), VillageCourier()], 9.5) == 90
