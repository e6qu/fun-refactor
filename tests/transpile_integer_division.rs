use fun_refactor::{lang::Language, transpile};
use serde_json::{json, Value};
use std::{fs, process::Command};

fn execute(source: &str, program: &str) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("subject.rs");
    fs::write(&original, source).unwrap();
    let translated = transpile::plan(&original, Language::Python).unwrap();
    assert_eq!(translated.fidelity.carried_verbatim, 0);
    let file = directory.path().join("subject.py");
    fs::write(&file, &translated.output).unwrap();
    let driver = format!(
        "import json\nnamespace = {{}}\nexec(open({}).read(), namespace)\n{program}\n",
        serde_json::to_string(file.to_str().unwrap()).unwrap()
    );
    let result = Command::new("python3")
        .args(["-I", "-B", "-c", &driver])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stderr),
        translated.output
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn large_signed_quotients_are_exact_and_float_division_keeps_its_fraction() {
    let pairs: [(i64, i64); 10] = [
        (7, 3),
        (-7, 3),
        (7, -3),
        (-7, -3),
        (i64::MIN, 1),
        (i64::MAX, 1),
        (i64::MAX, 2),
        (i64::MAX, 3),
        (i64::MIN, 3),
        (i64::MIN, i64::MAX),
    ];
    let source = "pub fn quotient(a: i64, b: i64) -> i64 { return a / b; }\n\
                  pub fn ratio(a: f64, b: f64) -> f64 { return a / b; }\n";
    let program = format!(
        "pairs = {}\nprint(json.dumps({{'integers': [namespace['quotient'](a, b) for a, b in pairs], 'float': namespace['ratio'](7.0, 2.0)}}))",
        serde_json::to_string(&pairs).unwrap()
    );
    let result = execute(source, &program);
    let expected: Vec<i64> = pairs.iter().map(|(a, b)| a / b).collect();
    assert_eq!(result["integers"], json!(expected));
    assert_eq!(result["float"], json!(3.5));
}

#[test]
fn division_evaluates_each_call_once_in_source_order() {
    let source = "pub fn left() -> i64 { return 7; }\n\
                  pub fn right() -> i64 { return 3; }\n\
                  pub fn quotient() -> i64 { return left() / right(); }\n";
    let program = "events = []\n\
                   def left():\n    events.append('left')\n    return -7\n\
                   def right():\n    events.append('right')\n    return 3\n\
                   namespace['left'] = left\nnamespace['right'] = right\n\
                   answer = namespace['quotient']()\nprint(json.dumps([answer, events]))";
    assert_eq!(execute(source, program), json!([-2, ["left", "right"]]));
}
