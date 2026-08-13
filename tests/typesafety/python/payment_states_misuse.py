# expect: fails
# title: receipt_id on a Pending payment, rejected by the checker
# misuse-of: payment_states
from dataclasses import dataclass


@dataclass(frozen=True)
class Pending:
    requested_at: str


@dataclass(frozen=True)
class Settled:
    requested_at: str
    receipt_id: str


type Payment = Pending | Settled


def receipt_of(payment: Payment) -> str:
    return payment.receipt_id  # error: Pending has no receipt_id
