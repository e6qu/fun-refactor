// expect: fails

declare const penceBrand: unique symbol;
type Pence = number & { readonly [penceBrand]: true };

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

type Line = { readonly item: string; readonly pence: Pence; readonly quantity: number };

function invoiceTotal(lines: readonly Line[]): Pence {
  return lines.reduce((sum, line) => sum + line.pence * line.quantity, 0) as Pence;
}

export function report(decoded: Json): Pence {
  return invoiceTotal(decoded);
}
