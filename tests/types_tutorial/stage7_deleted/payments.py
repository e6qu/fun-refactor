"""The checks for what can no longer happen, deleted."""

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
    """A whole number of the currency's smallest unit, and which currency that is.

    Built only through `of`, which is the one place a bad amount can be turned away.
    """

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


@dataclass(frozen=True)
class Initiated:
    id: PaymentId
    provider: Provider
    amount: Money


@dataclass(frozen=True)
class Authorized:
    id: PaymentId
    provider: Provider
    amount: Money


@dataclass(frozen=True)
class Captured:
    id: PaymentId
    provider: Provider
    amount: Money
    captured_at: int


@dataclass(frozen=True)
class Refunded:
    id: PaymentId
    provider: Provider
    amount: Money
    captured_at: int
    refunded: Money


@dataclass(frozen=True)
class Failed:
    id: PaymentId
    provider: Provider
    reason: str


type Payment = Initiated | Authorized | Captured | Refunded | Failed


@dataclass(frozen=True)
class Customer:
    id: CustomerId
    kyc: Kyc


@dataclass(frozen=True)
class VerifiedCustomer:
    """Only `verify` builds one, so holding one is proof the check was run."""

    id: CustomerId


@dataclass(frozen=True)
class Vendor:
    id: VendorId
    payouts: Payouts


@dataclass(frozen=True)
class PayoutEnabledVendor:
    """Only `enable_payouts` builds one. A vendor who cannot be paid has no value here."""

    id: VendorId


def verify(customer: Customer) -> VerifiedCustomer | None:
    if customer.kyc != Kyc.VERIFIED:
        return None
    return VerifiedCustomer(customer.id)


def enable_payouts(vendor: Vendor) -> PayoutEnabledVendor | None:
    if vendor.payouts != Payouts.ENABLED:
        return None
    return PayoutEnabledVendor(vendor.id)


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


def authorize(payment: Initiated, customer: VerifiedCustomer | None) -> Authorized | Failed:
    if payment.amount.exceeds(LIMIT_WITHOUT_VERIFICATION) and customer is None:
        return Failed(payment.id, payment.provider, "verification required")
    return Authorized(payment.id, payment.provider, payment.amount)


def capture(payment: Authorized, vendor: PayoutEnabledVendor) -> Captured:
    return Captured(payment.id, payment.provider, payment.amount, 1700000000)


def refund(payment: Captured, amount: Money) -> Refunded | Captured:
    if amount.exceeds(payment.amount):
        return payment
    return Refunded(
        payment.id, payment.provider, payment.amount, payment.captured_at, amount
    )
