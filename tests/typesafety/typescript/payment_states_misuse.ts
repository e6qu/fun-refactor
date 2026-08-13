// expect: fails

type Pending = { kind: "pending"; requestedAt: string };
type Settled = { kind: "settled"; requestedAt: string; receiptId: string };
type Payment = Pending | Settled;

export function receiptOf(payment: Payment): string {
  return payment.receiptId; // error: Pending has no receiptId
}
