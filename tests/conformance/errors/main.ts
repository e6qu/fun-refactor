function check(n: number): number {
    if (n < 0) {
        throw new Error("negative");
    }
    return n * 2;
}

function double(n: number): number {
    return check(n) + 1;
}

function main(): void {
    try {
        const v = check(5);
        console.log(`checked 5 -> ${v}`);
    } catch (e) {
        console.log(`caught ${(e as Error).message}`);
    }
    try {
        const v = check(-3);
        console.log(`never ${v}`);
    } catch (e) {
        console.log(`caught ${(e as Error).message}`);
    }
    try {
        const v = double(4);
        console.log(`double 4 -> ${v}`);
    } catch (e) {
        console.log(`caught ${(e as Error).message}`);
    }
    try {
        const v = double(-2);
        console.log(`never ${v}`);
    } catch (e) {
        console.log(`caught ${(e as Error).message}`);
    }
    console.log("done");
}

main();
