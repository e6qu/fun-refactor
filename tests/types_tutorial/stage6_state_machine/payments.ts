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

export interface Initiated {
  readonly kind: "initiated";
  readonly id: PaymentId;
  readonly provider: Provider;
  readonly amount: Money;
}

export interface Authorized {
  readonly kind: "authorized";
  readonly id: PaymentId;
  readonly provider: Provider;
  readonly amount: Money;
}

export interface Captured {
  readonly kind: "captured";
  readonly id: PaymentId;
  readonly provider: Provider;
  readonly amount: Money;
  readonly capturedAt: number;
}

export interface Refunded {
  readonly kind: "refunded";
  readonly id: PaymentId;
  readonly provider: Provider;
  readonly amount: Money;
  readonly capturedAt: number;
  readonly refunded: Money;
}

export interface Failed {
  readonly kind: "failed";
  readonly id: PaymentId;
  readonly provider: Provider;
  readonly reason: string;
}

export type Payment = Initiated | Authorized | Captured | Refunded | Failed;

export interface Customer {
  readonly id: CustomerId;
  readonly kyc: Kyc;
}

export interface VerifiedCustomer {
  readonly id: CustomerId;
}

export interface Vendor {
  readonly id: VendorId;
  readonly payouts: Payouts;
}

export interface PayoutEnabledVendor {
  readonly id: VendorId;
}

export function verify(customer: Customer): VerifiedCustomer | undefined {
  if (customer.kyc !== "verified") return undefined;
  return { id: customer.id };
}

export function enablePayouts(vendor: Vendor): PayoutEnabledVendor | undefined {
  if (vendor.payouts !== "enabled") return undefined;
  return { id: vendor.id };
}

const PROVIDER_VOCABULARY: Record<Provider, Record<string, PaymentState>> = {
  stripe: { requires_capture: "authorized", succeeded: "captured", canceled: "failed" },
  adyen: { Authorised: "authorized", SettleScheduled: "captured", Refused: "failed" },
  wise: { pending: "authorized", completed: "captured", failed: "failed" },
};

export function providerState(
  provider: Provider,
  raw: string,
): PaymentState | undefined {
  return PROVIDER_VOCABULARY[provider][raw];
}

export function authorize(
  payment: Initiated,
  customer: VerifiedCustomer | undefined,
): Authorized | Failed {
  if (payment.amount.exceeds(LIMIT_WITHOUT_VERIFICATION) && customer === undefined) {
    return {
      kind: "failed",
      id: payment.id,
      provider: payment.provider,
      reason: "verification required",
    };
  }
  return {
    kind: "authorized",
    id: payment.id,
    provider: payment.provider,
    amount: payment.amount,
  };
}

export function capture(
  payment: Authorized,
  vendor: PayoutEnabledVendor,
): Captured {
  if (payment.kind !== "authorized") throw new Error("not authorized");
  if (vendor === undefined) throw new Error("vendor cannot receive payouts");
  return {
    kind: "captured",
    id: payment.id,
    provider: payment.provider,
    amount: payment.amount,
    capturedAt: 1700000000,
  };
}

export function refund(payment: Captured, amount: Money): Refunded | Captured {
  if (payment.kind !== "captured") throw new Error("not captured");
  if (amount.exceeds(payment.amount)) return payment;
  return {
    kind: "refunded",
    id: payment.id,
    provider: payment.provider,
    amount: payment.amount,
    capturedAt: payment.capturedAt,
    refunded: amount,
  };
}
