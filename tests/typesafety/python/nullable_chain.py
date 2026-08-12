# expect: passes
# title: Absence written into the type
"""Every step can come back empty. `| None` makes each caller decide what
that means, and the ladder of checks is the cost."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Customer:
    referrer_id: str | None


def find_customer(customer_id: str) -> Customer | None:
    if customer_id == "c1":
        return Customer(referrer_id="c2")
    return None


def referrer_of(customer_id: str) -> Customer | None:
    customer = find_customer(customer_id)
    if customer is None:
        return None
    if customer.referrer_id is None:
        return None
    return find_customer(customer.referrer_id)
