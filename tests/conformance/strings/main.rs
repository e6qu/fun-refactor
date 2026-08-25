fn main() {
    let word = "Hello".to_string();
    println!("upper {}", word.to_uppercase());
    println!("lower {}", word.to_lowercase());
    println!("len {}", word.len());
    let joined = format!("{}-{}", word, "World");
    println!("concat {}", joined);
    if word.contains("ell") {
        println!("has yes");
    }
    if word.contains("xyz") {
        println!("never");
    } else {
        println!("has no");
    }
    println!("done");
}
