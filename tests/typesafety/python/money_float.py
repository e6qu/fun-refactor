# expect: passes
# title: A price and a discount rate are both bare numbers, so the swapped call is accepted
def apply_discount(total_pounds: float, rate: float) -> float:
    return total_pounds * (1 - rate)


def checkout() -> float:
    return apply_discount(0.1, 12.5)
