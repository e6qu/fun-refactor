// expect: passes
// A brand guards substitution and arithmetic walks around it. Both units are
// numbers underneath, so the checker accepts the sum and the brand is gone.

declare const secondsBrand: unique symbol;
declare const kilogramsBrand: unique symbol;

type Seconds = number & { readonly [secondsBrand]: true };
type Kilograms = number & { readonly [kilogramsBrand]: true };

export function nonsense(duration: Seconds, load: Kilograms): number {
  // The checker accepts this line. The result means nothing.
  return duration + load;
}
