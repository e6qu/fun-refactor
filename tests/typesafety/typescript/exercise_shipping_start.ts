// expect: passes

function shippingCents(
  weight: number,
  distance: number,
  express: boolean,
  insured: boolean,
  fragile: boolean,
): number {
  const rate = express ? 3 : 1;
  const surcharge = (insured ? 25 : 0) + (fragile ? 40 : 0);
  return Math.trunc(weight * distance * rate) + surcharge;
}

export const quoteA = shippingCents(2.5, 120.0, true, false, true);
export const quoteB = shippingCents(120.0, 2.5, true, true, false);
