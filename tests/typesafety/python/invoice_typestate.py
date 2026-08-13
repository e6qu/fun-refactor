# expect: passes
# run: yes
# title: send returns the only invoice that record_payment accepts
# improves: invoice_lifecycle_runtime
from dataclasses import dataclass


@dataclass(frozen=True)
class DraftInvoice:
    number: str

    def send(self) -> "SentInvoice":
        return SentInvoice(self.number)


@dataclass(frozen=True)
class SentInvoice:
    number: str

    def record_payment(self) -> "PaidInvoice":
        return PaidInvoice(self.number)


@dataclass(frozen=True)
class PaidInvoice:
    number: str


paid = DraftInvoice("INV-7").send().record_payment()
assert paid == PaidInvoice("INV-7")
