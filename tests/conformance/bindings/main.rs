fn main() {
    println!("start");
    let n = 42;
    let mut total = n + 10;
    println!("n {}", n);
    println!("sum {}", total);
    total = total * 2;
    println!("twice {}", total);
    let q = 10 / 3;
    let r = 10 % 3;
    println!("q {} r {}", q, r);
    let label = format!("item-{}", 7);
    println!("label {}", label);
    let mut i = 0;
    while i < 3 {
        println!("tick {}", i);
        i = i + 1;
    }
    println!("done");
}
