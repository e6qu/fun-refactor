// expect: passes

type Logged<T> = { readonly value: T; readonly log: readonly string[] };

function andThen<T, U>(logged: Logged<T>, step: (value: T) => Logged<U>): Logged<U> {
  const result = step(logged.value);
  return { value: result.value, log: [...logged.log, ...result.log] };
}

function double(n: number): Logged<number> {
  return { value: n * 2, log: [`doubled ${n}`] };
}

function addTax(n: number): Logged<number> {
  return { value: n + Math.floor(n / 10), log: [`taxed ${n}`] };
}

export function total(n: number): Logged<number> {
  return andThen(andThen({ value: n, log: [] }, double), addTax);
}
