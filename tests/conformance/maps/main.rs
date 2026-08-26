use std::collections::HashMap;

fn main() {
    println!("start");
    let mut ages: HashMap<String, i64> = HashMap::new();
    ages.insert("ada".to_string(), 36);
    ages.insert("alan".to_string(), 41);
    ages.insert("grace".to_string(), 45);
    println!("size {}", ages.len());
    println!("ada {}", ages["ada"]);
    let mut total: i64 = 0;
    for name in ["ada", "alan", "grace"] {
        total = total + ages[name];
    }
    println!("total {}", total);
    println!("done");
}
