// expect: passes

function applyDiscount(totalPounds: number, rate: number): number {
  return totalPounds * (1 - rate);
}

export function checkout(): number {
  return applyDiscount(0.1, 12.5);
}
