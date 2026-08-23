RETRY_LIMIT = 3

def provider_state(provider, raw):
    if provider == "stripe":
        if raw == "requires_capture":
            return "authorized"
        if raw == "succeeded":
            return "captured"
        if raw == "canceled":
            return "failed"
    if provider == "adyen":
        if raw == "Authorised":
            return "authorized"
        if raw == "SettleScheduled":
            return "captured"
        if raw == "Refused":
            return "failed"
    if provider == "wise":
        if raw == "pending":
            return "authorized"
        if raw == "completed":
            return "captured"
        if raw == "failed":
            return "failed"
    return "unknown"

def find_payment(payments, payment_id):
    return payments.get(payment_id)

def authorize(payment, customer):
    if payment["amount"] > 100000 and customer["kyc"] != "verified":
        return {"ok": False, "reason": "verification required"}
    payment["state"] = "authorized"
    return {"ok": True}

def capture(payment, vendor):
    if payment["state"] != "authorized":
        return {"ok": False, "reason": "not authorized"}
    if vendor["payouts"] != "enabled":
        return {"ok": False, "reason": "vendor cannot receive payouts"}
    payment["state"] = "captured"
    payment["captured_at"] = 1700000000
    return {"ok": True}

def refund(payment, amount):
    if payment["state"] != "captured":
        return {"ok": False, "reason": "not captured"}
    if amount > payment["amount"]:
        return {"ok": False, "reason": "refund exceeds capture"}
    payment["state"] = "refunded"
    return {"ok": True}
