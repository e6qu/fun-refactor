// expect: passes
// Ten cents, three times, is thirty cents. The float sum misses it.
// The Python twin runs this arithmetic in CI; the doubles are the same.

function totalPrice(prices: number[]): number {
  return prices.reduce((sum, price) => sum + price, 0);
}

// Three items at ten cents each.
export const sumIsOff = totalPrice([0.1, 0.1, 0.1]) !== 0.3; // true
