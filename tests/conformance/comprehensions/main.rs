fn main() {
    println!("start");
    let nums = vec![1, 2, 3, 4];
    let doubled = nums.iter().map(|n| n * 2).collect::<Vec<_>>();
    println!("first {}", doubled[0]);
    let mut total = 0;
    for d in &doubled {
        total = total + d;
    }
    println!("total {}", total);
    let big = nums.iter().filter(|n| **n > 2).map(|n| *n).collect::<Vec<_>>();
    let mut kept = 0;
    for b in &big {
        kept = kept + b;
    }
    println!("kept {}", kept);
    println!("done");
}
