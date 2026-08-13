// expect: passes

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
  return addPrices(19.99, "USD", 5.0, "EUR");
}
