function main(): void {
    console.log("start");
    const n = 42;
    let total = n + 10;
    console.log(`n ${n}`);
    console.log(`sum ${total}`);
    total = total * 2;
    console.log(`twice ${total}`);
    const q = Math.trunc(10 / 3);
    const r = 10 % 3;
    console.log(`q ${q} r ${r}`);
    const label = `item-${7}`;
    console.log(`label ${label}`);
    let i = 0;
    while (i < 3) {
        console.log(`tick ${i}`);
        i = i + 1;
    }
    console.log("done");
}

main();
