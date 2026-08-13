// expect: fails

type Pending = { readonly kind: "pending"; readonly requestedAt: string };
type Settled = { readonly kind: "settled"; readonly requestedAt: string; readonly receiptId: string };
type Payment = Pending | Settled;

export function receiptOf(payment: Payment): string {
  return payment.receiptId;
}
