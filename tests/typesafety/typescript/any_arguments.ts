// expect: passes

function invoiceLine(description: any, pricePence: any, quantity: any, taxed: any): any {
  const note = taxed ? " +tax" : "";
  return `${description} x${quantity} at ${pricePence}d${note}`;
}

export const line = invoiceLine(80, "handlebar grip", true, 2); // accepted, and wrong at run time
