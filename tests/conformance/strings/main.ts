function main(): void {
    const word = "Hello";
    console.log(`upper ${word.toUpperCase()}`);
    console.log(`lower ${word.toLowerCase()}`);
    console.log(`len ${word.length}`);
    const joined = word + "-" + "World";
    console.log(`concat ${joined}`);
    if (word.includes("ell")) {
        console.log("has yes");
    }
    if (word.includes("xyz")) {
        console.log("never");
    } else {
        console.log("has no");
    }
    console.log("done");
}

main();
