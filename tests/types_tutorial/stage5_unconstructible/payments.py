from dataclasses import dataclass
from enum import StrEnum
from typing import NewType

PaymentId = NewType("PaymentId", str)
CustomerId = NewType("CustomerId", str)
VendorId = NewType("VendorId", str)

RETRY_LIMIT: int = 3

class Provider(StrEnum):
    STRIPE = "stripe"
    ADYEN = "adyen"
    WISE = "wise"

class PaymentState(StrEnum):
    AUTHORIZED = "authorized"
    CAPTURED = "captured"
    REFUNDED = "refunded"
    FAILED = "failed"

class Currency(StrEnum):
    USD = "USD"
    EUR = "EUR"

class Kyc(StrEnum):
    UNVERIFIED = "unverified"
    PENDING = "pending"
    VERIFIED = "verified"

class Payouts(StrEnum):
    DISABLED = "disabled"
    ENABLED = "enabled"

@dataclass(frozen=True)
class Money:

    minor_units: int
    currency: Currency

    @staticmethod
    def of(minor_units: int, currency: Currency) -> "Money":
        if minor_units < 0:
            raise ValueError("an amount is never negative")
        return Money(minor_units, currency)

    def __add__(self, other: "Money") -> "Money":
        if self.currency != other.currency:
            raise ValueError("two currencies are not one amount")
        return Money(self.minor_units + other.minor_units, self.currency)

    def exceeds(self, other: "Money") -> bool:
        if self.currency != other.currency:
            raise ValueError("two currencies do not compare")
        return self.minor_units > other.minor_units

LIMIT_WITHOUT_VERIFICATION = Money(100000, Currency.USD)

@dataclass
class Payment:
    id: PaymentId
    provider: Provider
    amount: Money
    state: PaymentState
    captured_at: int | None = None

@dataclass
class Customer:
    id: CustomerId
    kyc: Kyc

@dataclass
class Vendor:
    id: VendorId
    payouts: Payouts

@dataclass(frozen=True)
class Outcome:
    ok: bool
    reason: str = ""

PROVIDER_VOCABULARY: dict[Provider, dict[str, PaymentState]] = {
    Provider.STRIPE: {
        "requires_capture": PaymentState.AUTHORIZED,
        "succeeded": PaymentState.CAPTURED,
        "canceled": PaymentState.FAILED,
    },
    Provider.ADYEN: {
        "Authorised": PaymentState.AUTHORIZED,
        "SettleScheduled": PaymentState.CAPTURED,
        "Refused": PaymentState.FAILED,
    },
    Provider.WISE: {
        "pending": PaymentState.AUTHORIZED,
        "completed": PaymentState.CAPTURED,
        "failed": PaymentState.FAILED,
    },
}

def provider_state(provider: Provider, raw: str) -> PaymentState | None:
    return PROVIDER_VOCABULARY[provider].get(raw)

def find_payment(payments: dict[PaymentId, Payment], payment_id: PaymentId) -> Payment | None:
    return payments.get(payment_id)

def authorize(payment: Payment, customer: Customer) -> Outcome:
    if payment.amount.exceeds(LIMIT_WITHOUT_VERIFICATION) and customer.kyc != Kyc.VERIFIED:
        return Outcome(False, "verification required")
    payment.state = PaymentState.AUTHORIZED
    return Outcome(True)

def capture(payment: Payment, vendor: Vendor) -> Outcome:
    if payment.state != PaymentState.AUTHORIZED:
        return Outcome(False, "not authorized")
    if vendor.payouts != Payouts.ENABLED:
        return Outcome(False, "vendor cannot receive payouts")
    payment.state = PaymentState.CAPTURED
    payment.captured_at = 1700000000
    return Outcome(True)

def refund(payment: Payment, amount: Money) -> Outcome:
    if payment.state != PaymentState.CAPTURED:
        return Outcome(False, "not captured")
    if amount.exceeds(payment.amount):
        return Outcome(False, "refund exceeds capture")
    payment.state = PaymentState.REFUNDED
    return Outcome(True)
