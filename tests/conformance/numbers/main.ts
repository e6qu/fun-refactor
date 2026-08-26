function floorDiv(a: number, b: number): number {
  const quotient = Math.trunc(a / b);
  if (a % b !== 0 && (a < 0) !== (b < 0)) {
    return quotient - 1;
  }
  return quotient;
}

function floorMod(a: number, b: number): number {
  return a - floorDiv(a, b) * b;
}

function main(): void {
  console.log("start");
  const a = 7;
  const b = 2;
  console.log(`sum ${a + b}`);
  console.log(`diff ${a - b}`);
  console.log(`product ${a * b}`);
  console.log(`quotient ${floorDiv(a, b)}`);
  console.log(`remainder ${floorMod(a, b)}`);
  const negative = -7;
  console.log(`negquotient ${floorDiv(negative, b)}`);
  console.log(`negremainder ${floorMod(negative, b)}`);
  console.log("done");
}

main();
