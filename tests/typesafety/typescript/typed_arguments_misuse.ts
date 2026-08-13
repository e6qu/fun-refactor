// expect: fails

function invoiceLine(description: string, pricePence: number, quantity: number, taxed: boolean): string {
  const note = taxed ? " +tax" : "";
  return `${description} x${quantity} at ${pricePence}d${note}`;
}

export const line = invoiceLine(80, "handlebar grip", true, 2); // rejected: all four arguments flagged
