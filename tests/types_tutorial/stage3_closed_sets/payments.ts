// The strings become closed sets, and each provider's vocabulary stops at the door.

declare const brand: unique symbol;
type Branded<T, B> = T & { readonly [brand]: B };

export type PaymentId = Branded<string, "PaymentId">;
export type CustomerId = Branded<string, "CustomerId">;
export type VendorId = Branded<string, "VendorId">;

export const RETRY_LIMIT: number = 3;

export type Provider = "stripe" | "adyen" | "wise";

export type PaymentState = "authorized" | "captured" | "refunded" | "failed";

const PROVIDER_VOCABULARY: Record<Provider, Record<string, PaymentState>> = {
  stripe: {
    requires_capture: "authorized",
    succeeded: "captured",
    canceled: "failed",
  },
  adyen: {
    Authorised: "authorized",
    SettleScheduled: "captured",
    Refused: "failed",
  },
  wise: {
    pending: "authorized",
    completed: "captured",
    failed: "failed",
  },
};

/** A word this provider does not use is not a state; it is unread input. */
export function providerState(
  provider: Provider,
  raw: string,
): PaymentState | undefined {
  return PROVIDER_VOCABULARY[provider][raw];
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
