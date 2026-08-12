# expect: fails
# title: Reading a receipt from a pending payment fails to compile
# misuse-of: payment_states
"""`receipt_id` exists only on `Settled`, and the checker says so."""

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
