// expect: passes

declare const penceBrand: unique symbol;
type Pence = number & { readonly [penceBrand]: true };

type InvoiceLine = { readonly item: string; readonly pence: Pence; readonly quantity: number };

function pickingList(lines: readonly InvoiceLine[], key: (...args: any[]) => number): InvoiceLine[] {
  return [...lines].sort((a, b) => key(a) - key(b));
}

export const cheapestFirst = pickingList([], (line, currency) => line[currency]);
