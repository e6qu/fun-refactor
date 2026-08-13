// expect: passes

export function invoiceLine(description: string, pricePence: number, quantity: number, taxed: boolean): string {
  const note = taxed ? " +tax" : "";
  return `${description} x${quantity} at ${pricePence}d${note}`;
}

export const grips = invoiceLine("handlebar grip", 80, 2, false); // "handlebar grip x2 at 80d"
export const saddle = invoiceLine("saddle", 155, 1, true); // "saddle x1 at 155d +tax"
