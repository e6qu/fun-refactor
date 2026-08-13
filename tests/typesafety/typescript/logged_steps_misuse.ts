// expect: fails

type Logged<T> = { readonly value: T; readonly log: readonly string[] };

function auditedTotal(n: number): Logged<number> {
  return { value: n + Math.floor(n / 10), log: [`taxed ${n}`] };
}

export function net(n: number): number {
  return auditedTotal(n) - 45;
}
