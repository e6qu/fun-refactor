// expect: passes

type Pending = { readonly kind: "pending"; readonly requestedAt: string };
type Settled = { readonly kind: "settled"; readonly requestedAt: string; readonly receiptId: string };
type Payment = Pending | Settled;

function assertNever(value: never): never {
  throw new Error(`unhandled case: ${JSON.stringify(value)}`);
}

export function describe(payment: Payment): string {
  switch (payment.kind) {
    case "pending":
      return `waiting since ${payment.requestedAt}`;
    case "settled":
      return `settled, receipt ${payment.receiptId}`;
    default:
      return assertNever(payment);
  }
}
