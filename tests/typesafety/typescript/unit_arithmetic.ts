// expect: passes
// A brand stops the wrong substitution, and arithmetic escapes it: `+` on two
// branded numbers is number + number again, and the brand is gone. TypeScript
// has no operator overloading, so route sums through typed functions.

declare const secondsBrand: unique symbol;
declare const kilogramsBrand: unique symbol;

type Seconds = number & { readonly [secondsBrand]: true };
type Kilograms = number & { readonly [kilogramsBrand]: true };

function seconds(n: number): Seconds {
  return n as Seconds;
}

function kilograms(n: number): Kilograms {
  return n as Kilograms;
}

function addSeconds(a: Seconds, b: Seconds): Seconds {
  return (a + b) as Seconds;
}

function addKilograms(a: Kilograms, b: Kilograms): Kilograms {
  return (a + b) as Kilograms;
}

export function totalWait(first: Seconds, second: Seconds): Seconds {
  return addSeconds(first, second);
}

export const load = addKilograms(kilograms(2.5), kilograms(1.5));
