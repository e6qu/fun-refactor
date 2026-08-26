fn floor_div(a: i64, b: i64) -> i64 {
    let quotient = a / b;
    match a % b != 0 && (a < 0) != (b < 0) {
        true => quotient - 1,
        false => quotient,
    }
}

fn floor_mod(a: i64, b: i64) -> i64 {
    a - floor_div(a, b) * b
}

fn main() {
    println!("start");
    let a = 7;
    let b = 2;
    println!("sum {}", a + b);
    println!("diff {}", a - b);
    println!("product {}", a * b);
    println!("quotient {}", floor_div(a, b));
    println!("remainder {}", floor_mod(a, b));
    let negative = -7;
    println!("negquotient {}", floor_div(negative, b));
    println!("negremainder {}", floor_mod(negative, b));
    println!("done");
}
