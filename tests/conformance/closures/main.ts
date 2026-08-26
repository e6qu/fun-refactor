function applyTo(f: (n: number) => number, n: number): number {
  return f(n);
}

function twice(f: (n: number) => number, n: number): number {
  return f(f(n));
}

function main(): void {
  console.log("start");
  const addOne = (n: number): number => n + 1;
  console.log(`apply ${applyTo(addOne, 6)}`);
  console.log(`twice ${twice(addOne, 10)}`);
  console.log("done");
}

main();
