// expect: passes
// A pending payment has no receipt. As two members of a discriminated union,
// that is a fact the checker knows, and `switch` must handle both.

type Pending = { kind: "pending"; requestedAt: string };
type Settled = { kind: "settled"; requestedAt: string; receiptId: string };
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
