//! Translating a file into another programming language.
//!
//! The promise is narrow and has to be tested as such: the *signature* is carried
//! exactly, the declarations are idiomatic in the target, and everything with no
//! counterpart is in the output verbatim rather than dropped or guessed at.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn translate(files: &[(&str, &str)], from: &str, to: Language) -> (String, transpile::Fidelity) {
    let (_tmp, root) = workspace(files);
    let plan = transpile::plan(&root.join(from), to).expect("a translation");
    (plan.output.clone(), plan.fidelity)
}

const RUST_SOURCE: &str = "\
/// Whether a reading can be stored.
pub fn validate(sensor: String, celsius: f64, limit: f64) -> bool {
    if celsius > limit {
        return false;
    }
    return true;
}

pub struct Reading {
    pub sensor: String,
    pub celsius: f64,
}
";

#[test]
fn a_signature_survives_the_crossing() {
    // The whole promise in one test: every parameter, in order, with its type, and
    // the return type — in the target's spelling and nothing else changed.
    for (target, expected) in [
        (
            Language::Python,
            "def validate(sensor: str, celsius: float, limit: float) -> bool:",
        ),
        (
            Language::Go,
            "func Validate(sensor string, celsius float64, limit float64) bool {",
        ),
        (
            Language::TypeScript,
            "export function validate(sensor: string, celsius: number, limit: number): boolean {",
        ),
    ] {
        let (output, fidelity) = translate(&[("a.rs", RUST_SOURCE)], "a.rs", target);
        assert!(
            output.contains(expected),
            "{target} should carry the signature exactly.\nwanted: {expected}\ngot:\n{output}"
        );
        assert_eq!(
            fidelity.signatures_complete, 1,
            "{target}: every type in this signature is one the IR knows"
        );
    }
}

#[test]
fn a_record_is_written_the_way_the_target_writes_records() {
    // Idiom, not transliteration: a dataclass in Python, a struct in Go, an interface
    // in TypeScript, because that is what each language calls a named product.
    let (python, _) = translate(&[("a.rs", RUST_SOURCE)], "a.rs", Language::Python);
    assert!(python.contains("@dataclass"), "got:\n{python}");
    assert!(python.contains("class Reading:"), "got:\n{python}");
    assert!(python.contains("sensor: str"), "got:\n{python}");

    let (go, _) = translate(&[("a.rs", RUST_SOURCE)], "a.rs", Language::Go);
    assert!(go.contains("type Reading struct {"), "got:\n{go}");
    assert!(go.contains("Sensor string"), "got:\n{go}");

    let (ts, _) = translate(&[("a.rs", RUST_SOURCE)], "a.rs", Language::TypeScript);
    assert!(ts.contains("interface Reading {"), "got:\n{ts}");
    assert!(ts.contains("sensor: string;"), "got:\n{ts}");
}

#[test]
fn control_flow_and_bindings_carry() {
    let source = "\
def total(values: list[int]) -> int:
    result = 0
    for value in values:
        if value > 0:
            result = result + value
        else:
            continue
    return result
";
    let (rust, fidelity) = translate(&[("a.py", source)], "a.py", Language::Rust);
    for wanted in [
        "fn total(values: Vec<i64>) -> i64 {",
        "for value in values {",
        "if value > 0 {",
        "} else {",
        "continue;",
        "return result;",
    ] {
        assert!(rust.contains(wanted), "missing `{wanted}` in:\n{rust}");
    }
    assert_eq!(
        fidelity.carried_verbatim, 0,
        "nothing in this function needs carrying:\n{rust}"
    );
}

#[test]
fn what_cannot_be_translated_is_in_the_output_verbatim() {
    // The other half of the promise. A closure and a macro have no counterpart in
    // Python; the result must contain the original text, not a silent gap.
    let source = "\
pub fn shout(names: Vec<String>) -> Vec<String> {
    let loud: Vec<String> = names.iter().map(|n| n.to_uppercase()).collect();
    return loud;
}
";
    let (python, fidelity) = translate(&[("a.rs", source)], "a.rs", Language::Python);
    assert!(fidelity.carried_verbatim > 0, "got:\n{python}");
    assert!(
        python.contains(transpile::MARKER),
        "carried code must be marked:\n{python}"
    );
    assert!(
        python.contains("names.iter().map(|n| n.to_uppercase()).collect()"),
        "the original must be in the file, not merely counted:\n{python}"
    );
    // And the signature still crossed, which is the point of translating at all.
    assert!(
        python.contains("def shout(names: list[str]) -> list[str]:"),
        "got:\n{python}"
    );
}

#[test]
fn an_interpolated_string_keeps_interpolating() {
    // `f"{c} below"` flattened to the literal text `{c} below` is not a gap, it is a
    // wrong answer — and it was one this found in its own output. Each target spells
    // interpolation its own way and every one of them must still substitute.
    let source = "\
def describe(celsius: float) -> str:
    return f\"{celsius} below the floor\"
";
    for (target, expected) in [
        (Language::Go, "fmt.Sprintf(\"%v below the floor\", celsius)"),
        (Language::TypeScript, "`${celsius} below the floor`"),
        (Language::Rust, "format!(\"{} below the floor\", celsius)"),
    ] {
        let (output, _) = translate(&[("a.py", source)], "a.py", target);
        assert!(
            output.contains(expected),
            "{target} should interpolate.\nwanted: {expected}\ngot:\n{output}"
        );
        // And never as literal text with the braces still in it.
        let literal: Vec<&str> = output
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains("{celsius}") && !l.contains("${celsius}"))
            .collect();
        assert!(
            literal.is_empty(),
            "{target} left the interpolation as text: {literal:?}"
        );
    }
}

#[test]
fn typed_python_and_typed_typescript_round_trip() {
    // The pair this is best at, and the one worth stating a standard for: a typed
    // Python module through TypeScript and back should lose nothing.
    let source = "\
from dataclasses import dataclass


@dataclass
class User:
    name: str
    email: str | None
    tags: list[str]


def active_names(users: list[User]) -> list[str]:
    names = [u.name for u in users if u.email is not None]
    return names


async def fetch(url: str, timeout: float = 5.0) -> dict[str, str]:
    result = {\"url\": url}
    return result
";
    let (typescript, forward) = translate(&[("a.py", source)], "a.py", Language::TypeScript);
    assert_eq!(
        forward.carried_verbatim, 0,
        "nothing here should need carrying:\n{typescript}"
    );
    for wanted in [
        "export interface User {",
        "email: string | null;",
        "tags: string[];",
        "export function activeNames(users: User[]): string[] {",
        "users.filter((u) => u.email !== null).map((u) => u.name)",
        "export async function fetch(url: string, timeout: number = 5.0): Promise<Record<string, string>> {",
    ] {
        assert!(typescript.contains(wanted), "missing `{wanted}`:\n{typescript}");
    }

    // And back, which is where a lossy step shows.
    let (python, back) = translate(&[("b.ts", typescript.as_str())], "b.ts", Language::Python);
    assert_eq!(
        back.carried_verbatim, 0,
        "the return trip should lose nothing either:\n{python}"
    );
    for wanted in [
        "@dataclass",
        "class User:",
        "email: str | None",
        "tags: list[str]",
        "def active_names(users: list[User]) -> list[str]:",
        "[u.name for u in users if u.email is not None]",
        "async def fetch(url: str, timeout: float = 5.0) -> dict[str, str]:",
    ] {
        assert!(python.contains(wanted), "missing `{wanted}`:\n{python}");
    }
}

#[test]
fn a_foreign_type_is_never_renamed_to_suit_the_target() {
    // `Reading` is a real type somewhere. Re-casing it to a convention would point
    // the signature at something that does not exist.
    let source = "pub fn first(items: Vec<HttpResponse>) -> HttpResponse {\n    todo!()\n}\n";
    for target in [Language::Python, Language::Go, Language::TypeScript] {
        let (output, fidelity) = translate(&[("a.rs", source)], "a.rs", target);
        assert!(
            output.contains("HttpResponse"),
            "{target} renamed a foreign type:\n{output}"
        );
        assert_eq!(
            fidelity.signatures_with_foreign_types, 1,
            "{target} must count a signature it cannot fully check"
        );
    }
}

#[test]
fn screaming_snake_constants_keep_their_words() {
    let source = "MIN_CELSIUS = -80.0\n";
    let (go, _) = translate(&[("a.py", source)], "a.py", Language::Go);
    assert!(
        go.contains("MinCelsius"),
        "`MIN_CELSIUS` must not become `MINCELSIUS`:\n{go}"
    );
}

#[test]
fn every_output_parses_as_the_language_it_claims_to_be() {
    // The strongest check available without a compiler: whatever comes out must be a
    // file the target's own grammar accepts.
    let sources = [
        ("a.rs", RUST_SOURCE),
        (
            "b.py",
            "def f(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    y = x + 1\n    return y\n",
        ),
        (
            "c.go",
            "package main\n\n// Add two things.\nfunc Add(a int, b int) int {\n\treturn a + b\n}\n",
        ),
        (
            "d.ts",
            "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
        ),
        (
            "E.java",
            "public class E {\n    /** Add two things. */\n    public int add(int a, int b) {\n             \treturn a + b;\n    }\n}\n",
        ),
        (
            "f.zig",
            "/// Add two things.\npub fn add(a: i64, b: i64) i64 {\n    return a + b;\n}\n",
        ),
    ];
    let (_tmp, root) = workspace(&sources);
    let parsers = fun_refactor::parse::Parsers::new();

    let mut checked = 0;
    for (name, _) in sources {
        let from = fun_refactor::lang::detect(&root.join(name)).unwrap();
        for to in transpile::SUPPORTED {
            if *to == from {
                continue;
            }
            let plan = transpile::plan(&root.join(name), *to)
                .unwrap_or_else(|e| panic!("{name} -> {to}: {e}"));
            let parsed = parsers
                .parse(*to, &plan.output)
                .unwrap_or_else(|e| panic!("{name} -> {to}: {e}"));
            assert!(
                !parsed.has_errors(),
                "{name} -> {to} produced something {to} cannot parse:\n{}",
                plan.output
            );
            checked += 1;
        }
    }
    // The count is written down so that adding a language without adding a source for
    // it fails here rather than quietly testing a fraction of the matrix.
    let languages = transpile::SUPPORTED.len();
    assert_eq!(
        checked,
        languages * (languages - 1),
        "every ordered pair should have been exercised"
    );
}

#[test]
fn the_real_sample_files_translate_into_something_that_parses() {
    // Toy inputs prove very little. These are the files the playground ships, and one
    // of them is what found `-> Result<(), String>` being written into a Python
    // annotation that Python cannot read.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/sample");
    let sources = [
        "src/ingest.rs",
        "src/main.rs",
        "src/convert.rs",
        "scripts/report.py",
        "cmd/collector.go",
        "cmd/sink.go",
        "web/dashboard.ts",
    ];

    let mut checked = 0;
    for name in sources {
        let path = root.join(name);
        let from = fun_refactor::lang::detect(&path).expect("a known language");
        for to in transpile::SUPPORTED {
            if *to == from {
                continue;
            }
            // `plan` verifies its own output; an unparseable result is an error here.
            match transpile::plan(&path, *to) {
                Ok(plan) => {
                    assert!(!plan.output.is_empty(), "{name} -> {to} produced nothing");
                    checked += 1;
                }
                Err(e) => {
                    let message = e.to_string();
                    // The only acceptable refusal is a destination that already
                    // exists, which the sample has for some pairs.
                    assert!(
                        message.contains("already exists"),
                        "{name} -> {to}: {message}"
                    );
                }
            }
        }
    }
    assert!(
        checked >= 15,
        "expected most pairs to translate, got {checked}"
    );
}

#[test]
fn translating_into_a_language_with_no_writer_is_refused() {
    let (_tmp, root) = workspace(&[("a.rs", RUST_SOURCE)]);
    let error = transpile::plan(&root.join("a.rs"), Language::Yaml).expect_err("no writer");
    assert!(
        error.to_string().contains("no writer"),
        "the refusal should say what is missing: {error}"
    );
}

/// A class with a method, in each source language, spelled that language's way.
const METHODS: &[(&str, &str)] = &[
    (
        "m.ts",
        "export class Repo {\n  name: string;\n\n  label(prefix: string): string {\n    \
         return this.name;\n  }\n}\n",
    ),
    (
        "m.py",
        "class Repo:\n    name: str\n\n    def label(self, prefix: str) -> str:\n        \
         return self.name\n",
    ),
    (
        "m.rs",
        "pub struct Repo {\n    pub name: String,\n}\n\nimpl Repo {\n    \
         pub fn label(&self, prefix: String) -> String {\n        return self.name;\n    }\n}\n",
    ),
    (
        "m.go",
        "package main\n\ntype Repo struct {\n\tName string\n}\n\n\
         func (r *Repo) Label(prefix string) string {\n\treturn r.Name\n}\n",
    ),
    (
        "m.zig",
        "pub const Repo = struct {\n    name: []const u8,\n\n    \
         pub fn label(self: Repo, prefix: []const u8) []const u8 {\n        \
         return self.name;\n    }\n};\n",
    ),
];

#[test]
fn the_receiver_is_spelled_the_way_the_target_spells_it() {
    // Six languages, and the receiver is not in the parameter list to be renamed with
    // the rest: Rust, Python and Zig say `self`, Java and TypeScript say `this`, and Go
    // says whatever the author called it. Every body used to keep its *source's* word,
    // so a translated method referred to a name the output never binds — `this.cache`
    // inside a Rust `impl` is not a typo, it is a file that cannot compile.
    for (name, source) in METHODS {
        let from = fun_refactor::lang::detect(std::path::Path::new(name)).unwrap();
        for to in transpile::SUPPORTED {
            if *to == from {
                continue;
            }
            let (output, _) = translate(&[(name, source)], name, *to);
            // Go capitalises an exported field, so the search has to be about the
            // word rather than its spelling.
            let body = output
                .lines()
                .find(|l| l.contains("return") && l.to_lowercase().contains("name"))
                .unwrap_or_else(|| panic!("{name} -> {to} lost the body:\n{output}"));
            let expected = match to {
                Language::Java | Language::TypeScript | Language::Tsx => "this.",
                _ => "self.",
            };
            assert!(
                body.contains(expected),
                "{name} -> {to} should reach the field through `{expected}`, not:\n{body}"
            );
            // And whatever the body reaches through, the signature has to bind.
            let bound = expected.trim_end_matches('.');
            assert!(
                *to == Language::Java
                    || *to == Language::TypeScript
                    || *to == Language::Tsx
                    || output.contains(bound),
                "{name} -> {to} uses `{bound}` without binding it:\n{output}"
            );
        }
    }
}

#[test]
fn a_typescript_class_member_is_public_unless_it_says_otherwise() {
    // The opposite of what a free function does, and reading both the same way made
    // every translated method private in Java and unreachable everywhere else — while
    // making every `private` field public, which is the same mistake pointing the
    // other way.
    let source = "export class Guard {\n  private secret: string;\n  token: string;\n\n  \
                  check(t: string): boolean {\n    return t === this.secret;\n  }\n\n  \
                  private hash(t: string): string {\n    return t;\n  }\n}\n";
    let (output, _) = translate(&[("g.ts", source)], "g.ts", Language::Java);
    assert!(
        output.contains("private String secret;"),
        "a private field should stay private:\n{output}"
    );
    assert!(
        output.contains("public String token;"),
        "a plain field is public:\n{output}"
    );
    assert!(
        output.contains("public boolean check("),
        "a plain method is public:\n{output}"
    );
    assert!(
        output.contains("private String hash("),
        "a private method should stay private:\n{output}"
    );
}

#[test]
fn zig_says_var_only_where_something_writes() {
    // Zig rejects a `var` nothing writes to, and only the Rust reader records
    // mutability at all — every other one says "mutable" because it has nothing better
    // to say. Taking that at its word turned a `const` file into one that will not
    // build.
    let source = "def tally(xs: list[int]) -> int:\n    total = 0\n    label = \"n\"\n    \
                  for x in xs:\n        total = total + x\n    return total\n";
    let (output, _) = translate(&[("t.py", source)], "t.py", Language::Zig);
    assert!(
        output.contains("var total"),
        "`total` is written to in the loop:\n{output}"
    );
    assert!(
        output.contains("const label"),
        "`label` is never written to:\n{output}"
    );
}

#[test]
fn zig_carries_what_it_cannot_say_above_the_statement() {
    // Zig is the only target here with no block comment: `//` runs to the end of the
    // line, so a carried fragment written beside an expression would swallow the rest
    // of the statement, semicolon included.
    let source = "def greet(name: str) -> str:\n    line = f\"hi {name}\"\n    return line\n";
    let (output, fidelity) = translate(&[("g.py", source)], "g.py", Language::Zig);
    assert_eq!(fidelity.carried_verbatim, 1, "{output}");
    let at = output
        .find("const line")
        .unwrap_or_else(|| panic!("no binding:\n{output}"));
    let before = output[..at].trim_end();
    assert!(
        before.lines().last().unwrap().trim().starts_with("//"),
        "the carried text belongs on its own line above:\n{output}"
    );
    assert!(
        output.contains("const line = undefined;"),
        "the statement has to survive whole:\n{output}"
    );
}

#[test]
fn a_word_zig_reserves_is_still_the_name_the_source_wrote() {
    // Go's `error` is Zig's keyword for an error set, and a signature returning one did
    // not parse. `@"error"` is how Zig writes an identifier that collides with one of
    // its own words, and under it the name still says what the source said.
    let source = "package main\n\nfunc Check(a int) error {\n\treturn nil\n}\n";
    let (output, _) = translate(&[("c.go", source)], "c.go", Language::Zig);
    assert!(
        output.contains("@\"error\""),
        "a reserved word carried across is escaped, not renamed:\n{output}"
    );
}
