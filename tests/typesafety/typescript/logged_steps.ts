// expect: passes
// `Logged` pairs a value with the log that produced it. `andThen` runs the
// next step and concatenates the trails, so the log arrives with the answer,
// as data. This shape is usually called the Writer monad.

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
// total(100) is { value: 220, log: ["doubled 100", "taxed 200"] }.
// The Python twin runs these assertions in CI.
