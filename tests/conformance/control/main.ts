function classify(n: number): string {
    if (n < 0) {
        return "negative";
    } else if (n == 0) {
        return "zero";
    } else if (n < 10) {
        return "small";
    }
    return "large";
}

function main(): void {
    console.log(`classify -5 ${classify(-5)}`);
    console.log(`classify 0 ${classify(0)}`);
    console.log(`classify 7 ${classify(7)}`);
    console.log(`classify 40 ${classify(40)}`);
    let i = 0;
    while (i < 6) {
        i = i + 1;
        if (i % 2 == 0) {
            continue;
        }
        if (i == 5) {
            break;
        }
        console.log(`odd ${i}`);
    }
    for (const value of [3, 1, 2]) {
        console.log(`item ${value}`);
    }
    let outer = 0;
    while (outer < 3) {
        let inner = 0;
        while (inner < 3) {
            if (inner == 2) {
                break;
            }
            console.log(`pair ${outer} ${inner}`);
            inner = inner + 1;
        }
        outer = outer + 1;
    }
    console.log("done");
}

main();
