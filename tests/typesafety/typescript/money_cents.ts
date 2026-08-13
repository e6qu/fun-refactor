// expect: passes

function totalCents(pricesCents: number[]): number {
  return pricesCents.reduce((sum, price) => sum + price, 0);
}

// The same three items.
export const sumIsExact = totalCents([10, 10, 10]) === 30; // true
