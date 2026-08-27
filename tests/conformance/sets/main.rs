use std::collections::HashSet;

fn main() {
    println!("start");
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert("ada".to_string());
    seen.insert("alan".to_string());
    seen.insert("ada".to_string());
    println!("size {}", seen.len());
    if seen.contains("ada") {
        println!("has-ada yes");
    } else {
        println!("has-ada no");
    }
    if seen.contains("grace") {
        println!("has-grace yes");
    } else {
        println!("has-grace no");
    }
    seen.remove("alan");
    println!("after {}", seen.len());
    println!("done");
}
