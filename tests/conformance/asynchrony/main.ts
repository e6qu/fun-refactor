async function load(name: string, base: number): Promise<number> {
    console.log(`fetch ${name}`);
    return base + 1;
}

async function total(a: number, b: number): Promise<number> {
    const first = await load("a", a);
    const second = await load("b", b);
    return first + second;
}

async function main(): Promise<void> {
    console.log("start");
    const result = await total(10, 20);
    console.log(`total ${result}`);
    console.log("done");
}

main();
