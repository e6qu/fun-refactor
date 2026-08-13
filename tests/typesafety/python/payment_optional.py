# expect: passes
# title: This Payment type allows a settled payment with no receipt
from dataclasses import dataclass


@dataclass(frozen=True)
class Payment:
    requested_at: str
    settled: bool
    receipt_id: str | None


impossible_a = Payment("09:00", settled=True, receipt_id=None)
impossible_b = Payment("09:00", settled=False, receipt_id="r-42")
