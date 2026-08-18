//! The whole-project gate: a package translated as a directory builds as a unit.
//!
//! A directory sweep translates every file against the merged context of the
//! whole set. The cheap assertions here need no toolchain and hold the seams.
//! An import of a sibling becomes a real import. A name declared in one file
//! is spelled the same way where another file uses it.
//!
//! With the toolchains installed the gate goes further. The TypeScript the
//! sweep writes must satisfy `tsc --strict --noEmit` with zero errors. Each
//! translated entrypoint must print byte for byte what its source printed.
//! A machine without the tools skips and says so; on CI a skip fails, the same
//! rule `tests/typesafety.rs` follows.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ------------------------------------------------------------------ fixtures

/// A four-file Python package: models, storage, helpers and an entrypoint.
///
/// Every seam a project has is in it. `cli` constructs classes two other files
/// declare, reads a property and calls a two-word method across files, and
/// every file imports siblings by name.
const PYTHON_PACKAGE: &[(&str, &str)] = &[
    (
        "models.py",
        r#""""Inventory domain models."""


class Item:
    def __init__(self, sku: str, name: str, price_cents: int, qty: int = 0) -> None:

        self.sku = sku
        self.name = name
        self.price_cents = price_cents
        self.qty = qty

    @property
    def total_cents(self) -> int:
        return self.price_cents * self.qty

    def restock(self, amount: int) -> None:
        self.qty = self.qty + amount

    def describe(self) -> str:
        return self.sku + " " + self.name + " x" + str(self.qty)
"#,
    ),
    (
        "storage.py",
        r#""""A store that keeps its items in a list."""

from models import Item


class Store:
    def __init__(self, items: list[Item]) -> None:
        self.items = items

    def add(self, item: Item) -> None:
        self.items.append(item)

    def count(self) -> int:
        return len(self.items)

    def total_value_cents(self) -> int:
        total = 0

        for item in self.items:
            total = total + item.total_cents

        return total
"#,
    ),
    (
        "helpers.py",
        r#""""Formatting helpers."""


def banner(title: str) -> str:
    return "== " + title.upper() + " =="


def format_cents(cents: int) -> str:
    return str(cents) + "c"
"#,
    ),
    (
        "cli.py",
        r#""""Entry point that drives the store."""

from helpers import banner, format_cents
from models import Item
from storage import Store


def build_store() -> Store:
    store = Store([])
    store.add(Item("A1", "apple", 150, 10))
    store.add(Item("B2", "banana", 75, 6))
    store.add(Item("C3", "cherry", 300))
    return store


def main() -> None:
    print(banner("inventory"))
    store = build_store()

    for item in store.items:
        print(item.describe() + " = " + format_cents(item.total_cents))

    print("items: " + str(store.count()))
    print("total: " + format_cents(store.total_value_cents()))


if __name__ == "__main__":
    main()
"#,
    ),
];

/// The same program as a four-file TypeScript package.
///
/// The imports carry `.ts` extensions so that node can run the sources as the
/// baseline; the sweep resolves them to the same siblings either way.
const TYPESCRIPT_PACKAGE: &[(&str, &str)] = &[
    (
        "models.ts",
        r#"export class Item {
    sku: string;
    name: string;
    priceCents: number;
    qty: number;

    constructor(sku: string, name: string, priceCents: number, qty: number = 0) {

        this.sku = sku;
        this.name = name;
        this.priceCents = priceCents;
        this.qty = qty;
    }

    get totalCents(): number {
        return this.priceCents * this.qty;
    }

    describe(): string {
        return this.sku + " " + this.name + " x" + String(this.qty);
    }
}
"#,
    ),
    (
        "storage.ts",
        r#"import { Item } from "./models.ts";

export class Store {
    items: Item[];

    constructor(items: Item[]) {
        this.items = items;
    }

    add(item: Item): void {
        this.items.push(item);
    }

    count(): number {
        return this.items.length;
    }

    totalValueCents(): number {
        let total = 0;

        for (const item of this.items) {
            total = total + item.totalCents;
        }

        return total;
    }
}
"#,
    ),
    (
        "helpers.ts",
        r#"export function banner(title: string): string {
    return "== " + title.toUpperCase() + " ==";
}

export function formatCents(cents: number): string {
    return String(cents) + "c";
}
"#,
    ),
    (
        "cli.ts",
        r#"import { banner, formatCents } from "./helpers.ts";
import { Item } from "./models.ts";
import { Store } from "./storage.ts";

export function buildStore(): Store {
    const store = new Store([]);
    store.add(new Item("A1", "apple", 150, 10));
    store.add(new Item("B2", "banana", 75, 6));
    store.add(new Item("C3", "cherry", 300));
    return store;
}

export function main(): void {
    console.log(banner("inventory"));
    const store = buildStore();

    for (const item of store.items) {
        console.log(item.describe() + " = " + formatCents(item.totalCents));
    }

    console.log("items: " + String(store.count()));
    console.log("total: " + formatCents(store.totalValueCents()));
}

main();
"#,
    ),
];

/// What both packages print. Held in one place so the two run gates cannot
/// drift apart, and asserted against both baselines before any comparison.
const EXPECTED_STDOUT: &str = "== INVENTORY ==\n\
    A1 apple x10 = 1500c\n\
    B2 banana x6 = 450c\n\
    C3 cherry x0 = 0c\n\
    items: 3\n\
    total: 1950c\n";

// ------------------------------------------------------------------ the sweep

/// Write a fixture package into `dir`.
fn write_package(dir: &Path, files: &[(&str, &str)]) {
    for (name, source) in files {
        std::fs::write(dir.join(name), source).expect("writing a fixture file");
    }
}

/// Translate every fixture file under `dir` into `to`, the way the directory
/// sweep does. It reads the whole set first, then plans each file against the
/// merged context. The outputs land under `out_dir` and come back by stem.
fn sweep(
    dir: &Path,
    files: &[(&str, &str)],
    to: Language,
    out_dir: &Path,
) -> BTreeMap<String, String> {
    let mut modules: BTreeMap<PathBuf, transpile::Module> = BTreeMap::new();
    for (name, _) in files {
        let path = dir.join(name);
        modules.insert(path.clone(), transpile::read_file(&path).expect("a module"));
    }
    let mut context = transpile::Module::default();
    for module in modules.values() {
        context.items.extend(module.items.iter().cloned());
    }
    let extension = match to {
        Language::TypeScript => "ts",
        Language::Python => "py",
        other => panic!("this gate has no package for {other}."),
    };
    std::fs::create_dir_all(out_dir).expect("the output directory");
    let mut outputs = BTreeMap::new();
    for (name, _) in files {
        let path = dir.join(name);
        let stem = path
            .file_stem()
            .expect("a stem")
            .to_string_lossy()
            .into_owned();
        let destination = out_dir.join(format!("{stem}.{extension}"));
        let plan =
            transpile::plan_to_in_context(&path, to, Some(&destination), false, &context, &modules)
                .expect("a translation");
        std::fs::write(&destination, &plan.output).expect("writing a translation");
        outputs.insert(stem, plan.output);
    }
    outputs
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// ------------------------------------------------------------------ the gates

#[test]
fn a_python_package_sweeps_to_typescript_with_real_imports_and_one_naming_table() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    write_package(tmp.path(), PYTHON_PACKAGE);
    let outputs = sweep(
        tmp.path(),
        PYTHON_PACKAGE,
        Language::TypeScript,
        &tmp.path().join("ts"),
    );

    let cli = &outputs["cli"];
    for line in [
        "import { banner, formatCents } from \"./helpers\";",
        "import { Item } from \"./models\";",
        "import { Store } from \"./storage\";",
    ] {
        assert!(cli.contains(line), "cli.ts lost a sibling import.\n{cli}");
    }
    assert!(
        cli.contains("new Store([])") && cli.contains("new Item(\"A1\", \"apple\", 150, 10)"),
        "a sibling's class must be constructed with `new`.\n{cli}"
    );
    assert!(
        cli.contains("formatCents(item.totalCents)"),
        "a sibling's property and function must take the target's casing.\n{cli}"
    );
    assert!(
        cli.contains("store.totalValueCents()"),
        "a sibling's method must be called as its declaration spells it.\n{cli}"
    );
    assert!(
        outputs["models"].contains("get totalCents(): number"),
        "the property must be declared under the same name its callers use.\n{}",
        outputs["models"]
    );
    for (stem, output) in &outputs {
        assert!(
            !output.contains("yours to add"),
            "{stem}.ts still carries a sibling import as a comment.\n{output}"
        );
    }
}

#[test]
fn a_typescript_package_sweeps_to_python_with_real_relative_imports() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    write_package(tmp.path(), TYPESCRIPT_PACKAGE);
    let outputs = sweep(
        tmp.path(),
        TYPESCRIPT_PACKAGE,
        Language::Python,
        &tmp.path().join("py"),
    );

    let cli = &outputs["cli"];
    for line in [
        "from .helpers import banner, format_cents",
        "from .models import Item",
        "from .storage import Store",
    ] {
        assert!(cli.contains(line), "cli.py lost a sibling import.\n{cli}");
    }
    assert!(
        cli.contains("format_cents(item.total_cents)"),
        "a sibling's property and function must take the target's casing.\n{cli}"
    );
    assert!(
        cli.contains("store.total_value_cents()"),
        "a sibling's method must be called as its declaration spells it.\n{cli}"
    );
    for (stem, output) in &outputs {
        assert!(
            !output.contains("yours to add"),
            "{stem}.py still carries a sibling import as a comment.\n{output}"
        );
    }
}

#[test]
fn the_typescript_a_sweep_writes_compiles_strictly_and_runs_like_its_python_source() {
    if !common::Toolchain::Tsc.is_available() {
        eprintln!("translate_projects: tsc is not installed, so the TypeScript went unchecked.");
        common::require_on_ci("the whole-project gate", &["tsc".to_string()]);
        return;
    }
    let tmp = tempfile::tempdir().expect("a temporary directory");
    write_package(tmp.path(), PYTHON_PACKAGE);
    let ts_dir = tmp.path().join("ts");
    sweep(tmp.path(), PYTHON_PACKAGE, Language::TypeScript, &ts_dir);

    let names: Vec<String> = PYTHON_PACKAGE
        .iter()
        .map(|(name, _)| name.replace(".py", ".ts"))
        .collect();
    let checked = Command::new("tsc")
        .current_dir(&ts_dir)
        .args(["--strict", "--noEmit", "--target", "es2022"])
        .args(["--module", "esnext", "--moduleResolution", "bundler"])
        .args(&names)
        .output()
        .expect("running tsc");
    assert!(
        checked.status.success(),
        "the translated package does not satisfy tsc --strict:\n{}",
        said(&checked)
    );

    if !common::Toolchain::Python.is_available() || !node_available() {
        eprintln!("translate_projects: python3 or node is missing, so nothing ran.");
        common::require_on_ci("the whole-project gate", &["python3 and node".to_string()]);
        return;
    }
    let baseline = Command::new("python3")
        .current_dir(tmp.path())
        .arg("cli.py")
        .output()
        .expect("running the python source");
    assert!(baseline.status.success(), "{}", said(&baseline));
    assert_eq!(
        String::from_utf8_lossy(&baseline.stdout),
        EXPECTED_STDOUT,
        "the python source no longer prints what this gate expects."
    );

    // Emitted as CommonJS, so node resolves the extensionless sibling
    // imports the way tsc did.
    let emitted = Command::new("tsc")
        .current_dir(&ts_dir)
        .args(["--strict", "--target", "es2022"])
        .args(["--module", "commonjs", "--outDir", "js"])
        .args(&names)
        .output()
        .expect("running tsc");
    assert!(emitted.status.success(), "{}", said(&emitted));
    let ran = Command::new("node")
        .current_dir(&ts_dir)
        .arg("js/cli.js")
        .output()
        .expect("running node");
    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        EXPECTED_STDOUT,
        "the translated entrypoint prints something else than its source."
    );
}

#[test]
fn the_python_a_sweep_writes_runs_like_its_typescript_source() {
    if !common::Toolchain::Python.is_available() || !node_available() {
        eprintln!("translate_projects: python3 or node is missing, so nothing ran.");
        common::require_on_ci("the whole-project gate", &["python3 and node".to_string()]);
        return;
    }
    let tmp = tempfile::tempdir().expect("a temporary directory");
    write_package(tmp.path(), TYPESCRIPT_PACKAGE);
    // Node needs to know the sources are modules; the sweep never reads this.
    std::fs::write(
        tmp.path().join("package.json"),
        "{ \"type\": \"module\" }\n",
    )
    .expect("writing package.json");
    let baseline = Command::new("node")
        .current_dir(tmp.path())
        .arg("cli.ts")
        .output()
        .expect("running the typescript source");
    if !baseline.status.success() {
        // Node strips types only from 22.6 on, and an older node is not a
        // defect in the translation. On CI the baseline has to run.
        eprintln!("translate_projects: node cannot run .ts sources, so nothing ran.");
        common::require_on_ci("the whole-project gate", &["node >= 22.6".to_string()]);
        return;
    }
    assert_eq!(
        String::from_utf8_lossy(&baseline.stdout),
        EXPECTED_STDOUT,
        "the typescript source no longer prints what this gate expects."
    );

    // The translation writes relative imports, so it runs as a package.
    let pkg = tmp.path().join("pkg");
    sweep(tmp.path(), TYPESCRIPT_PACKAGE, Language::Python, &pkg);
    std::fs::write(pkg.join("__init__.py"), "").expect("writing __init__.py");
    let ran = Command::new("python3")
        .current_dir(tmp.path())
        .args(["-m", "pkg.cli"])
        .output()
        .expect("running python3");
    assert!(ran.status.success(), "{}", said(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        EXPECTED_STDOUT,
        "the translated entrypoint prints something else than its source."
    );
}

#[test]
fn a_name_two_files_declare_is_renamed_where_the_directory_is_one_namespace() {
    // Two Python modules may each declare a `Thing`. Go puts every file of a
    // directory in one package, so the sweep produced `Thing redeclared in
    // this block` and reported both files translated.
    let files = &[
        (
            "a.py",
            "class Thing:\n    def label(self) -> str:\n        return \"a-thing\"\n",
        ),
        (
            "c.py",
            "class Thing:\n    def label(self) -> str:\n        return \"c-thing\"\n",
        ),
    ];
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("pkg");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in files {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let mut modules: BTreeMap<PathBuf, transpile::Module> = BTreeMap::new();
    for (name, _) in files {
        let path = dir.join(name);
        modules.insert(path.clone(), transpile::read_file(&path).expect("a module"));
    }
    let mut context = transpile::Module::default();
    for module in modules.values() {
        context.items.extend(module.items.iter().cloned());
    }
    let out_dir = tmp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut written = BTreeMap::new();
    for (name, _) in files {
        let path = dir.join(name);
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let destination = out_dir.join(format!("{stem}.go"));
        let plan = transpile::plan_to_in_context(
            &path,
            Language::Go,
            Some(&destination),
            false,
            &context,
            &modules,
        )
        .expect("a translation");
        written.insert(stem, plan.output);
    }
    let first = &written["a"];
    let second = &written["c"];
    assert!(
        first.contains("type Thing struct"),
        "the file earliest by path keeps the plain name.\n{first}"
    );
    assert!(
        second.contains("type CThing struct") && !second.contains("type Thing struct"),
        "the other takes its own file's name in front.\n{second}"
    );
    assert!(
        second.contains("is declared by another file of this sweep"),
        "and the header says so.\n{second}"
    );
}
