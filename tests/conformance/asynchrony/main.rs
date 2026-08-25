fn load(name: &str, base: i64) -> i64 {
    println!("fetch {}", name);
    base + 1
}

fn total(a: i64, b: i64) -> i64 {
    let first = load("a", a);
    let second = load("b", b);
    first + second
}

fn main() {
    println!("start");
    let result = total(10, 20);
    println!("total {}", result);
    println!("done");
}
