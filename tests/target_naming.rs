//! Naming the thing you want to act on.

use assert_cmd::Command;
use predicates::str::contains;

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(path, content).expect("the file");
    }
    tmp
}

const TWO_CLASSES: &str = "\
class Box:
    def size(self):
        return 1

class Crate:
    def size(self):
        return 2

def go(b, c):
    return b.size() + c.size()
";

#[test]
fn a_qualified_name_selects_the_symbol() {
    let tmp = workspace(&[("a.py", TWO_CLASSES)]);
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["callers", "Box::size", "-C"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(contains("Box::size"));
}

#[test]
fn the_name_the_listing_prints_is_the_name_that_works() {
    // The two are the same string or this is a trap.
    let tmp = workspace(&[("a.py", TWO_CLASSES)]);
    let listing = Command::cargo_bin("fr")
        .expect("the binary")
        .args(["symbols", "--json", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    let printed: serde_json::Value =
        serde_json::from_slice(&listing.stdout).expect("symbols emits json");
    let qualified: Vec<String> = printed
        .as_array()
        .expect("an array")
        .iter()
        .filter(|s| s["name"] == "size")
        .map(|s| s["qualified_name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(qualified.len(), 2, "got {qualified:?}");

    for name in qualified {
        Command::cargo_bin("fr")
            .expect("the binary")
            .args(["callers", &name, "-C"])
            .arg(tmp.path())
            .assert()
            .success();
    }
}

#[test]
fn an_ambiguous_bare_name_offers_the_names_that_are_not() {
    // Before, this said "specify a position as path:line:col" and listed the bare name
    // twice, which tells a reader the answer is somewhere else and does not say where.
    let tmp = workspace(&[("a.py", TWO_CLASSES)]);
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["callers", "size", "-C"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(contains("Box::size"))
        .stderr(contains("Crate::size"))
        .stderr(contains("name one of these"));
}

#[test]
fn a_qualified_name_that_is_still_ambiguous_says_where() {
    // Two packages may both declare `Box`, and then the qualified name is not enough either.
    let tmp = workspace(&[("one/a.py", TWO_CLASSES), ("two/b.py", TWO_CLASSES)]);
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["callers", "Box::size", "-C"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(contains("one/a.py"))
        .stderr(contains("two/b.py"));
}

#[test]
fn a_name_nothing_declares_is_still_refused() {
    let tmp = workspace(&[("a.py", TWO_CLASSES)]);
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["callers", "Nowhere::at_all", "-C"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(contains("no symbol named"));
}
