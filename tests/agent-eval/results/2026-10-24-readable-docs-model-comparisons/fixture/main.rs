mod subject;
fn main() {
    let result = subject::allowed(false, false, false);
    println!("{result}");
    let result = subject::allowed(false, false, true);
    println!("{result}");
    let result = subject::allowed(false, true, false);
    println!("{result}");
    let result = subject::allowed(false, true, true);
    println!("{result}");
    let result = subject::allowed(true, false, false);
    println!("{result}");
    let result = subject::allowed(true, false, true);
    println!("{result}");
    let result = subject::allowed(true, true, false);
    println!("{result}");
    let result = subject::allowed(true, true, true);
    println!("{result}");
}
