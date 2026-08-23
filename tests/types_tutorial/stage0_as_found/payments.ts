export const RETRY_LIMIT = 3;

export function providerState(provider, raw) {
  if (provider === "stripe") {
    if (raw === "requires_capture") return "authorized";
    if (raw === "succeeded") return "captured";
    if (raw === "canceled") return "failed";
  }
  if (provider === "adyen") {
    if (raw === "Authorised") return "authorized";
    if (raw === "SettleScheduled") return "captured";
    if (raw === "Refused") return "failed";
  }
  if (provider === "wise") {
    if (raw === "pending") return "authorized";
    if (raw === "completed") return "captured";
    if (raw === "failed") return "failed";
  }
  return "unknown";
}

export function findPayment(payments, paymentId) {
  return payments[paymentId];
}

export function authorize(payment, customer) {
  if (payment.amount > 100000 && customer.kyc !== "verified") {
    return { ok: false, reason: "verification required" };
  }
  payment.state = "authorized";
  return { ok: true };
}

export function capture(payment, vendor) {
  if (payment.state !== "authorized") {
    return { ok: false, reason: "not authorized" };
  }
  if (vendor.payouts !== "enabled") {
    return { ok: false, reason: "vendor cannot receive payouts" };
  }
  payment.state = "captured";
  payment.capturedAt = 1700000000;
  return { ok: true };
}

export function refund(payment, amount) {
  if (payment.state !== "captured") {
    return { ok: false, reason: "not captured" };
  }
  if (amount > payment.amount) {
    return { ok: false, reason: "refund exceeds capture" };
  }
  payment.state = "refunded";
  return { ok: true };
}
