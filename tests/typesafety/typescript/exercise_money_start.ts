// expect: passes
// An amount is a number and its currency is a string beside it. Mixing
// currencies is caught at run time, at best.

function addPrices(
  amountA: number,
  currencyA: string,
  amountB: number,
  currencyB: string,
): number {
  if (currencyA !== currencyB) {
    throw new Error("cannot add different currencies");
  }
  return amountA + amountB;
}

export function basketTotal(): number {
  // Throws at run time. Nothing warned about it earlier.
  return addPrices(19.99, "USD", 5.0, "EUR");
}
