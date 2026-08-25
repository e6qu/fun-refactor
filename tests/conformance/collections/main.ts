function main(): void {
    const nums: number[] = [];
    nums.push(3);
    nums.push(1);
    nums.push(2);
    console.log(`len ${nums.length}`);
    console.log(`first ${nums[0]}`);
    nums[1] = 10;
    let total = 0;
    for (const value of nums) {
        total = total + value;
    }
    console.log(`sum ${total}`);
    for (const value of nums) {
        console.log(`item ${value}`);
    }
    console.log("done");
}

main();
