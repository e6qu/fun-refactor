// expect: passes

declare const kilogramsBrand: unique symbol;
declare const kilometersBrand: unique symbol;

type Kilograms = number & { readonly [kilogramsBrand]: true };
type Kilometers = number & { readonly [kilometersBrand]: true };

function kilograms(n: number): Kilograms {
  return n as Kilograms;
}

function kilometers(n: number): Kilometers {
  return n as Kilometers;
}

type Handling = {
  readonly express?: boolean;
  readonly insured?: boolean;
  readonly fragile?: boolean;
};

function shippingCents(weight: Kilograms, distance: Kilometers, handling: Handling): number {
  const rate = handling.express ? 3 : 1;
  const surcharge = (handling.insured ? 25 : 0) + (handling.fragile ? 40 : 0);
  return Math.trunc(weight * distance * rate) + surcharge;
}

export const quote = shippingCents(kilograms(2.5), kilometers(120.0), {
  express: true,
  fragile: true,
});
