// expect: passes

function totalPounds(pricesPounds: readonly number[]): number {
  return pricesPounds.reduce((sum, price) => sum + price, 0);
}

// Two shillings is a tenth of a pound.
export const sumIsOff = totalPounds([0.1, 0.1, 0.1]) !== 0.3; // true
