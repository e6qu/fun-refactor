// expect: passes

declare const penceBrand: unique symbol;
type Pence = number & { readonly [penceBrand]: true };

type InvoiceLine = { readonly item: string; readonly pence: Pence; readonly gift: boolean };

function isGift(line: InvoiceLine): boolean {
  return line.gift;
}

function amount(line: InvoiceLine): Pence {
  return line.pence;
}

export function giftTotal(lines: readonly InvoiceLine[]): number {
  return lines.filter(isGift).map(amount).reduce((sum, pence) => sum + pence, 0);
}
