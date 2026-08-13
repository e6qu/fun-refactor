# expect: passes
# title: shipping_cents takes five positional arguments, three of them bool
def shipping_cents(
    weight: float, distance: float, express: bool, insured: bool, fragile: bool
) -> int:
    rate = 3 if express else 1
    surcharge = (25 if insured else 0) + (40 if fragile else 0)
    return int(weight * distance * rate) + surcharge


quote_a = shipping_cents(2.5, 120.0, True, False, True)
quote_b = shipping_cents(120.0, 2.5, True, True, False)
