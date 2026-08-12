// expect: passes
// The same IEEE 754 doubles as Python. The Python twin runs these sums in CI,
// so the claims in the comments are executed claims.

function totalAsFloat(prices: number[]): number {
  return prices.reduce((sum, price) => sum + price, 0);
}

function totalAsCents(pricesCents: number[]): number {
  return pricesCents.reduce((sum, price) => sum + price, 0);
}

// Three items at ten cents each.
export const floatIsOff = totalAsFloat([0.1, 0.1, 0.1]) !== 0.3; // true
export const centsAreExact = totalAsCents([10, 10, 10]) === 30; // true
