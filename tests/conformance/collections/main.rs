fn main() {
    let mut nums: Vec<i64> = Vec::new();
    nums.push(3);
    nums.push(1);
    nums.push(2);
    println!("len {}", nums.len());
    println!("first {}", nums[0]);
    nums[1] = 10;
    let mut total = 0;
    for value in &nums {
        total = total + value;
    }
    println!("sum {}", total);
    for value in &nums {
        println!("item {}", value);
    }
    println!("done");
}
