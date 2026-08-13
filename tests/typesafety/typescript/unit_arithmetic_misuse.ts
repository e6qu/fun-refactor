// expect: fails

declare const metersBrand: unique symbol;
declare const kilogramsBrand: unique symbol;

type Meters = number & { readonly [metersBrand]: true };
type Kilograms = number & { readonly [kilogramsBrand]: true };

function meters(n: number): Meters {
  return n as Meters;
}

function kilograms(n: number): Kilograms {
  return n as Kilograms;
}

function addMeters(a: Meters, b: Meters): Meters {
  return (a + b) as Meters;
}

export const nonsense = addMeters(meters(1.8), kilograms(4)); // error: Kilograms is not Meters
