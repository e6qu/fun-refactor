// expect: passes
// The same prices, held as integer cents. The sum is exact at any scale.

function totalCents(pricesCents: number[]): number {
  return pricesCents.reduce((sum, price) => sum + price, 0);
}

// The same three items.
export const sumIsExact = totalCents([10, 10, 10]) === 30; // true
