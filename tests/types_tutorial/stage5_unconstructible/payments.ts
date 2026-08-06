// An amount cannot be negative, and two currencies are never one number.

declare const brand: unique symbol;
type Branded<T, B> = T & { readonly [brand]: B };

export type PaymentId = Branded<string, "PaymentId">;
export type CustomerId = Branded<string, "CustomerId">;
export type VendorId = Branded<string, "VendorId">;

export const RETRY_LIMIT: number = 3;

export type Provider = "stripe" | "adyen" | "wise";
export type PaymentState = "authorized" | "captured" | "refunded" | "failed";
export type Currency = "USD" | "EUR";
export type Kyc = "unverified" | "pending" | "verified";
export type Payouts = "disabled" | "enabled";

/**
 * A whole number of the currency's smallest unit, and which currency that is.
 *
 * Built only through `Money.of`, which is the one place a bad amount can be
 * turned away.
 */
export class Money {
  private constructor(
    readonly minorUnits: number,
    readonly currency: Currency,
  ) {}

  static of(minorUnits: number, currency: Currency): Money {
    if (minorUnits < 0) throw new Error("an amount is never negative");
    return new Money(minorUnits, currency);
  }

  plus(other: Money): Money {
    if (this.currency !== other.currency) {
      throw new Error("two currencies are not one amount");
    }
    return Money.of(this.minorUnits + other.minorUnits, this.currency);
  }

  exceeds(other: Money): boolean {
    if (this.currency !== other.currency) {
      throw new Error("two currencies do not compare");
    }
    return this.minorUnits > other.minorUnits;
  }
}

export const LIMIT_WITHOUT_VERIFICATION = Money.of(100000, "USD");

export interface Payment {
  id: PaymentId;
  provider: Provider;
  amount: Money;
  state: PaymentState;
  capturedAt?: number;
}

export interface Customer {
  id: CustomerId;
  kyc: Kyc;
}

export interface Vendor {
  id: VendorId;
  payouts: Payouts;
}

export interface Outcome {
  readonly ok: boolean;
  readonly reason?: string;
}

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

export function providerState(
  provider: Provider,
  raw: string,
): PaymentState | undefined {
  return PROVIDER_VOCABULARY[provider][raw];
}

export function findPayment(
  payments: Record<PaymentId, Payment>,
  paymentId: PaymentId,
): Payment | undefined {
  return payments[paymentId];
}

export function authorize(payment: Payment, customer: Customer): Outcome {
  if (
    payment.amount.exceeds(LIMIT_WITHOUT_VERIFICATION) &&
    customer.kyc !== "verified"
  ) {
    return { ok: false, reason: "verification required" };
  }
  payment.state = "authorized";
  return { ok: true };
}

export function capture(payment: Payment, vendor: Vendor): Outcome {
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

export function refund(payment: Payment, amount: Money): Outcome {
  if (payment.state !== "captured") {
    return { ok: false, reason: "not captured" };
  }
  if (amount.exceeds(payment.amount)) {
    return { ok: false, reason: "refund exceeds capture" };
  }
  payment.state = "refunded";
  return { ok: true };
}
