function work(): void {
    console.log("open a");
    try {
        console.log("open b");
        try {
            console.log("work");
        } finally {
            console.log("close b");
        }
    } finally {
        console.log("close a");
    }
}

function main(): void {
    work();
    console.log("done");
}

main();
