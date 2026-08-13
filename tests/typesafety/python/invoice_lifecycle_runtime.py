# expect: passes
# title: Any invoice accepts any operation, and the order is checked only when it runs
from dataclasses import dataclass


@dataclass
class Invoice:
    number: str
    status: str

    def send(self) -> None:
        if self.status != "draft":
            raise ValueError("only a draft can be sent")
        self.status = "sent"

    def record_payment(self) -> None:
        if self.status != "sent":
            raise ValueError("only a sent invoice can be paid")
        self.status = "paid"


def rush(invoice: Invoice) -> None:
    invoice.record_payment()
    invoice.send()
