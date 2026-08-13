// expect: passes

declare const metersBrand: unique symbol;
declare const kilogramsBrand: unique symbol;

type Meters = number & { readonly [metersBrand]: true };
type Kilograms = number & { readonly [kilogramsBrand]: true };

export function nonsense(tubing: Meters, grease: Kilograms): number {
  return tubing + grease;
}
