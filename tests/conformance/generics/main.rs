struct Box {
    value: i64,
}

impl Box {
    fn get(&self) -> i64 {
        self.value
    }
}

fn first_of(items: Vec<i64>) -> i64 {
    items[0]
}

fn count_of(items: Vec<String>) -> i64 {
    items.len() as i64
}

fn main() {
    println!("start");
    let numbers = vec![4, 5, 6];
    let words = vec!["a".to_string(), "b".to_string()];
    println!("first {}", first_of(numbers));
    println!("count {}", count_of(words));
    let b = Box { value: 9 };
    println!("box {}", b.get());
    println!("done");
}
