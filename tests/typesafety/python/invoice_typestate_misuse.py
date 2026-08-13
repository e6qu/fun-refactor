# expect: fails
# title: Paying a draft that was never sent, rejected by the checker
# misuse-of: invoice_typestate
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


paid = DraftInvoice("INV-7").record_payment()
