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

def find_payment(payments: dict, payment_id: PaymentId) -> dict | None:
    return payments.get(payment_id)

def authorize(payment: dict, customer: dict) -> dict:
    if payment["amount"] > 100000 and customer["kyc"] != "verified":
        return {"ok": False, "reason": "verification required"}
    payment["state"] = PaymentState.AUTHORIZED
    return {"ok": True}

def capture(payment: dict, vendor: dict) -> dict:
    if payment["state"] != PaymentState.AUTHORIZED:
        return {"ok": False, "reason": "not authorized"}
    if vendor["payouts"] != "enabled":
        return {"ok": False, "reason": "vendor cannot receive payouts"}
    payment["state"] = PaymentState.CAPTURED
    payment["captured_at"] = 1700000000
    return {"ok": True}

def refund(payment: dict, amount: int) -> dict:
    if payment["state"] != PaymentState.CAPTURED:
        return {"ok": False, "reason": "not captured"}
    if amount > payment["amount"]:
        return {"ok": False, "reason": "refund exceeds capture"}
    payment["state"] = PaymentState.REFUNDED
    return {"ok": True}
