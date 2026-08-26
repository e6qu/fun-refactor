function main(): void {
  console.log("start");
  const seen = new Set<string>();
  seen.add("ada");
  seen.add("alan");
  seen.add("ada");
  console.log(`size ${seen.size}`);
  if (seen.has("ada")) {
    console.log("has-ada yes");
  } else {
    console.log("has-ada no");
  }
  if (seen.has("grace")) {
    console.log("has-grace yes");
  } else {
    console.log("has-grace no");
  }
  seen.delete("alan");
  console.log(`after ${seen.size}`);
  console.log("done");
}

main();
