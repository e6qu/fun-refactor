// expect: fails
// The sum of two different units means nothing, and now the checker says so.

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

export const nonsense = addSeconds(seconds(30), kilograms(4)); // error: Kilograms is not Seconds
