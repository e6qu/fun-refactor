function main(): void {
  console.log("start");
  const ages: Map<string, number> = new Map();
  ages.set("ada", 36);
  ages.set("alan", 41);
  ages.set("grace", 45);
  console.log(`size ${ages.size}`);
  console.log(`ada ${ages.get("ada")!}`);
  let total = 0;
  for (const name of ["ada", "alan", "grace"]) {
    total = total + ages.get(name)!;
  }
  console.log(`total ${total}`);
  console.log("done");
}

main();
