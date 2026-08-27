class Box {
  value: number;

  constructor(value: number) {
    this.value = value;
  }

  get(): number {
    return this.value;
  }
}

function firstOf(items: number[]): number {
  return items[0];
}

function countOf(items: string[]): number {
  return items.length;
}

function main(): void {
  console.log("start");
  const numbers = [4, 5, 6];
  const words = ["a", "b"];
  console.log(`first ${firstOf(numbers)}`);
  console.log(`count ${countOf(words)}`);
  const b = new Box(9);
  console.log(`box ${b.get()}`);
  console.log("done");
}

main();
