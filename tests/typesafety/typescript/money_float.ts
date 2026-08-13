// expect: passes

function totalPrice(prices: number[]): number {
  return prices.reduce((sum, price) => sum + price, 0);
}

// Three items at ten cents each.
export const sumIsOff = totalPrice([0.1, 0.1, 0.1]) !== 0.3; // true
