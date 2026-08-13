// expect: fails

declare const metersBrand: unique symbol;
declare const eachBrand: unique symbol;

type Meters = number & { readonly [metersBrand]: true };
type Each = number & { readonly [eachBrand]: true };

function each(n: number): Each {
  return n as Each;
}

function cutTubing(length: Meters): string {
  return `cutting ${length}m of tubing`;
}

export function restock(): string {
  const spokes = each(36);
  cutTubing(1.8);
  return cutTubing(spokes);
}
