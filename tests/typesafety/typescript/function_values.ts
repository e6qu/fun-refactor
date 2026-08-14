// expect: passes

declare const penceBrand: unique symbol;
type Pence = number & { readonly [penceBrand]: true };

type InvoiceLine = { readonly item: string; readonly pence: Pence; readonly quantity: number };

function pickingList(
  lines: readonly InvoiceLine[],
  key: (line: InvoiceLine) => number,
): InvoiceLine[] {
  return [...lines].sort((a, b) => key(a) - key(b));
}

const basket: InvoiceLine[] = [
  { item: "saddle", pence: 155 as Pence, quantity: 1 },
  { item: "bell", pence: 80 as Pence, quantity: 3 },
];

export const cheapestFirst = pickingList(basket, (line) => line.pence);
export const fewestFirst = pickingList(basket, (line) => line.quantity);
