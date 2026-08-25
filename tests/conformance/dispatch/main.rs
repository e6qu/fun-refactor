fn day_name(day: i64) -> String {
    match day {
        1 => "mon".to_string(),
        2 => "tue".to_string(),
        3 => "wed".to_string(),
        _ => "other".to_string(),
    }
}

fn op_kind(word: String) -> String {
    match word.as_str() {
        "add" => "plus".to_string(),
        "sub" => "minus".to_string(),
        _ => "other".to_string(),
    }
}

fn main() {
    println!("day 1 {}", day_name(1));
    println!("day 3 {}", day_name(3));
    println!("day 9 {}", day_name(9));
    println!("kind add {}", op_kind("add".to_string()));
    println!("kind sub {}", op_kind("sub".to_string()));
    println!("kind mul {}", op_kind("mul".to_string()));
    println!("done");
}
