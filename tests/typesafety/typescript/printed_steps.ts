// expect: passes
// Each step logs by printing. The order of lines depends on when the steps
// run, a test has to capture the console to see them, and a caller can
// neither inspect the trail nor attach it to the answer.

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
