// expect: passes

declare const metersBrand: unique symbol;

type Meters = number & { readonly [metersBrand]: true };

function meters(n: number): Meters {
  return n as Meters;
}

function cutTubing(length: Meters): string {
  return `cutting ${length}m of tubing`;
}

export function restock(): string {
  const frameTubing = meters(1.8);
  return cutTubing(frameTubing);
}
