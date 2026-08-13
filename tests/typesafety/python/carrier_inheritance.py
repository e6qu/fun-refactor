# expect: passes
# title: Every carrier must inherit from the base class, even the one you cannot edit
class Carrier:
    def quote_pence(self, kilograms: float) -> int:
        raise NotImplementedError


class RoyalPost(Carrier):
    def quote_pence(self, kilograms: float) -> int:
        return int(kilograms * 120)


def cheapest(carriers: list[Carrier], kilograms: float) -> int:
    return min(carrier.quote_pence(kilograms) for carrier in carriers)
