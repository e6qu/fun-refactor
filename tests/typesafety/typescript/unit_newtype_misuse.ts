// expect: fails
// The same call as before, with a plain `number` and with `Meters`. Both are rejected.

declare const secondsBrand: unique symbol;
declare const metersBrand: unique symbol;

type Seconds = number & { readonly [secondsBrand]: true };
type Meters = number & { readonly [metersBrand]: true };

function meters(n: number): Meters {
  return n as Meters;
}

function waitBeforeRetry(delay: Seconds): string {
  return `sleeping ${delay}s`;
}

export function plan(): string {
  const distance = meters(30);
  waitBeforeRetry(30); // error: number is not Seconds
  return waitBeforeRetry(distance); // error: Meters is not Seconds
}
