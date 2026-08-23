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

class Kyc(StrEnum):
    UNVERIFIED = "unverified"
    PENDING = "pending"
    VERIFIED = "verified"

class Payouts(StrEnum):
    DISABLED = "disabled"
    ENABLED = "enabled"

@dataclass
class Payment:
    id: PaymentId
    provider: Provider
    amount: int
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
    if payment.amount > 100000 and customer.kyc != Kyc.VERIFIED:
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

def refund(payment: Payment, amount: int) -> Outcome:
    if payment.state != PaymentState.CAPTURED:
        return Outcome(False, "not captured")
    if amount > payment.amount:
        return Outcome(False, "refund exceeds capture")
    payment.state = PaymentState.REFUNDED
    return Outcome(True)
