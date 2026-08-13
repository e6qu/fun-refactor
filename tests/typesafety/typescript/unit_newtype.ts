// expect: passes

declare const secondsBrand: unique symbol;

type Seconds = number & { readonly [secondsBrand]: true };

function seconds(n: number): Seconds {
  return n as Seconds;
}

function waitBeforeRetry(delay: Seconds): string {
  return `sleeping ${delay}s`;
}

export function plan(): string {
  const timeout = seconds(30);
  return waitBeforeRetry(timeout);
}
