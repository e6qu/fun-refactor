// expect: passes

function double(n: number): number {
  console.log(`doubled ${n}`);
  return n * 2;
}

function addTax(n: number): number {
  console.log(`taxed ${n}`);
  return n + Math.floor(n / 10);
}

export function total(n: number): number {
  return addTax(double(n));
}
