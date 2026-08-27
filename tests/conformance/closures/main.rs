fn apply_to(f: impl Fn(i64) -> i64, n: i64) -> i64 {
    f(n)
}

fn twice(f: impl Fn(i64) -> i64, n: i64) -> i64 {
    f(f(n))
}

fn main() {
    println!("start");
    let add_one = |n: i64| n + 1;
    println!("apply {}", apply_to(add_one, 6));
    println!("twice {}", twice(add_one, 10));
    println!("done");
}
