function main(): void {
  console.log("start");
  const nums = [1, 2, 3, 4];
  const doubled = nums.map((n: any) => n * 2);
  console.log(`first ${doubled[0]}`);
  let total = 0;
  for (const d of doubled) {
    total = total + d;
  }
  console.log(`total ${total}`);
  const big = nums.filter((n: any) => n > 2);
  let kept = 0;
  for (const b of big) {
    kept = kept + b;
  }
  console.log(`kept ${kept}`);
  console.log("done");
}

main();
