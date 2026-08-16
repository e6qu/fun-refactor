// expect: passes

declare const penceBrand: unique symbol;
type Pence = number & { readonly [penceBrand]: true };

type Line = { readonly item: string; readonly pence: Pence; readonly quantity: number };

function parseLine(raw: unknown): Line {
  if (
    typeof raw !== "object" ||
    raw === null ||
    !("item" in raw) ||
    !("pence" in raw) ||
    !("quantity" in raw) ||
    typeof raw.item !== "string" ||
    typeof raw.pence !== "number" ||
    typeof raw.quantity !== "number"
  ) {
    throw new Error("not a line");
  }
  return { item: raw.item, pence: raw.pence as Pence, quantity: raw.quantity };
}

function lineTotal(line: Line): Pence {
  return (line.pence * line.quantity) as Pence;
}

export function invoiceTotal(lines: readonly Line[]): Pence {
  return lines.reduce((sum, line) => sum + lineTotal(line), 0) as Pence;
}

export function isLarge(lines: readonly Line[]): boolean {
  return invoiceTotal(lines) > 1000;
}

export const basket = [parseLine({ item: "saddle", pence: 155, quantity: 4 })];
