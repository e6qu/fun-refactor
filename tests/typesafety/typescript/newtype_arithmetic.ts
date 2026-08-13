// expect: passes

declare const secondsBrand: unique symbol;
declare const kilogramsBrand: unique symbol;

type Seconds = number & { readonly [secondsBrand]: true };
type Kilograms = number & { readonly [kilogramsBrand]: true };

export function nonsense(duration: Seconds, load: Kilograms): number {
  return duration + load; // accepted, and the sum means nothing
}
