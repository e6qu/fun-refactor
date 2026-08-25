fn classify(n: i64) -> String {
    if n < 0 {
        return "negative".to_string();
    } else if n == 0 {
        return "zero".to_string();
    } else if n < 10 {
        return "small".to_string();
    }
    "large".to_string()
}

fn main() {
    println!("classify -5 {}", classify(-5));
    println!("classify 0 {}", classify(0));
    println!("classify 7 {}", classify(7));
    println!("classify 40 {}", classify(40));
    let mut i = 0;
    while i < 6 {
        i = i + 1;
        if i % 2 == 0 {
            continue;
        }
        if i == 5 {
            break;
        }
        println!("odd {}", i);
    }
    for value in [3, 1, 2] {
        println!("item {}", value);
    }
    let mut outer = 0;
    while outer < 3 {
        let mut inner = 0;
        while inner < 3 {
            if inner == 2 {
                break;
            }
            println!("pair {} {}", outer, inner);
            inner = inner + 1;
        }
        outer = outer + 1;
    }
    println!("done");
}
