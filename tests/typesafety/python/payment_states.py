# expect: passes
# run: yes
# title: Pending and Settled become separate types
# improves: payment_optional
from dataclasses import dataclass
from typing import assert_never


@dataclass(frozen=True)
class Pending:
    requested_at: str


@dataclass(frozen=True)
class Settled:
    requested_at: str
    receipt_id: str


type Payment = Pending | Settled


def describe(payment: Payment) -> str:
    match payment:
        case Pending(requested_at=at):
            return f"waiting since {at}"
        case Settled(receipt_id=receipt):
            return f"settled, receipt {receipt}"
        case _:
            assert_never(payment)


assert describe(Pending("09:00")) == "waiting since 09:00"
assert describe(Settled("09:00", "r-42")) == "settled, receipt r-42"
