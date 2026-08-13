// expect: passes

declare const metersBrand: unique symbol;
declare const squareMetersBrand: unique symbol;
declare const kilogramsBrand: unique symbol;

type Meters = number & { readonly [metersBrand]: true };
type SquareMeters = number & { readonly [squareMetersBrand]: true };
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

function timesMeters(a: Meters, b: Meters): SquareMeters {
  return (a * b) as SquareMeters;
}

function addKilograms(a: Kilograms, b: Kilograms): Kilograms {
  return (a + b) as Kilograms;
}

export function totalTubing(topTube: Meters, downTube: Meters): Meters {
  return addMeters(topTube, downTube);
}

export function chainGuardSheet(width: Meters, height: Meters): SquareMeters {
  return timesMeters(width, height);
}

export const tubing = totalTubing(meters(0.72), meters(1.02));

export const shippingWeight = addKilograms(kilograms(9.5), kilograms(2.1));
