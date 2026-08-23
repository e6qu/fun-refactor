declare const brand: unique symbol;
type Branded<T, B> = T & { readonly [brand]: B };

export type PaymentId = Branded<string, "PaymentId">;
export type CustomerId = Branded<string, "CustomerId">;
export type VendorId = Branded<string, "VendorId">;

export const RETRY_LIMIT: number = 3;

export function providerState(provider: string, raw: string): string {
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

export function findPayment(
  payments: Record<string, object>,
  paymentId: PaymentId,
): object | undefined {
  return payments[paymentId];
}

export function authorize(payment: any, customer: any): object {
  if (payment.amount > 100000 && customer.kyc !== "verified") {
    return { ok: false, reason: "verification required" };
  }
  payment.state = "authorized";
  return { ok: true };
}

export function capture(payment: any, vendor: any): object {
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

export function refund(payment: any, amount: number): object {
  if (payment.state !== "captured") {
    return { ok: false, reason: "not captured" };
  }
  if (amount > payment.amount) {
    return { ok: false, reason: "refund exceeds capture" };
  }
  payment.state = "refunded";
  return { ok: true };
}
