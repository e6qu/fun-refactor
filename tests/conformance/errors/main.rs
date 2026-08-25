fn check(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("negative".to_string());
    }
    Ok(n * 2)
}

fn double(n: i64) -> Result<i64, String> {
    Ok(check(n)? + 1)
}

fn main() {
    match check(5) {
        Ok(v) => println!("checked 5 -> {}", v),
        Err(e) => println!("caught {}", e),
    }
    match check(-3) {
        Ok(v) => println!("never {}", v),
        Err(e) => println!("caught {}", e),
    }
    match double(4) {
        Ok(v) => println!("double 4 -> {}", v),
        Err(e) => println!("caught {}", e),
    }
    match double(-2) {
        Ok(v) => println!("never {}", v),
        Err(e) => println!("caught {}", e),
    }
    println!("done");
}
