// expect: fails

declare const penceBrand: unique symbol;
type Pence = number & { readonly [penceBrand]: true };

type InvoiceLine = { readonly item: string; readonly pence: Pence; readonly gift: boolean };

function isGift(line: InvoiceLine): boolean {
  return line.gift;
}

function label(line: InvoiceLine): string {
  return line.item;
}

export function giftTotal(lines: readonly InvoiceLine[]): number {
  return lines.filter(isGift).map(label).reduce((sum, pence) => sum + pence, 0);
}
