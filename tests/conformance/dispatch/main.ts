function dayName(day: number): string {
    switch (day) {
        case 1:
            return "mon";
        case 2:
            return "tue";
        case 3:
            return "wed";
        default:
            return "other";
    }
}

function opKind(word: string): string {
    switch (word) {
        case "add":
            return "plus";
        case "sub":
            return "minus";
        default:
            return "other";
    }
}

function main(): void {
    console.log(`day 1 ${dayName(1)}`);
    console.log(`day 3 ${dayName(3)}`);
    console.log(`day 9 ${dayName(9)}`);
    console.log(`kind add ${opKind("add")}`);
    console.log(`kind sub ${opKind("sub")}`);
    console.log(`kind mul ${opKind("mul")}`);
    console.log("done");
}

main();
