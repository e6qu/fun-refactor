//! Does the code that a rewrite writes still compile?
//!
//! `output_compiles.rs` drives the commands that move a declaration — rename, signature,
//! move, inline. These are the ones that rewrite a declaration in place: `fr extract`
//! lifts an expression or a run of statements out, `fr rewrite` turns one shape into
//! another, and `fr restructure` does that to every occurrence at once. None of the three
//! had ever been compiled outside Rust.
//!
//! Every case starts from a fixture that compiles, applies exactly one refactoring, and
//! compiles again. A refusal passes: some of these are legitimately not available in a
//! given language, and the capability matrix says which. Writing a plan the compiler then
//! rejects is the only outcome forbidden.
//!
//! The fixtures are one per language, each holding every shape the three commands need, so
//! a language is added here once and gets all of them.

mod common;
use common::{gate, must_plan, GateRun, Toolchain, Workspace};

use fun_refactor::lang::Language;
use fun_refactor::refactor::rewrite::Rewrite;
use fun_refactor::span::Span;

/// One language's fixture: what compiles it, and the text to find each shape by.
struct Fixture {
    language: Language,
    toolchain: Toolchain,
    /// The file the shapes live in.
    file: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The expressions to lift into a binding, one sweep each. Chosen to sit in
    /// different places — a return, a condition, a call argument, and inside one — because
    /// where the binding has to go is what varies.
    expressions: &'static [&'static str],
    /// The run of statements to lift into a function, as the first and last of them.
    statements: Option<(&'static str, &'static str)>,
    /// Where each rewrite applies, or `None` where the fixture has no such shape.
    invert_if: Option<&'static str>,
    de_morgan: Option<&'static str>,
    guard_clause: Option<&'static str>,
    /// Shapes that occur more than once, and what each becomes. More than one, because
    /// what varies is the pattern: one metavariable or two, and a match nested inside
    /// another match of the same shape.
    restructures: &'static [(&'static str, &'static str, &'static str)],
}

impl Fixture {
    fn workspace(&self) -> Workspace {
        Workspace::with(self.toolchain, self.files)
    }

    /// The byte range of a piece of the fixture's text.
    fn span_of(&self, ws: &Workspace, needle: &str) -> Span {
        let source = ws.read(self.file);
        let at = source
            .parse_offset(needle)
            .unwrap_or_else(|| panic!("{} does not contain {needle:?}", self.file));
        Span::new(at, at + needle.len())
    }
}

/// `str::find`, named so the call sites read as what they mean.
trait ParseOffset {
    fn parse_offset(&self, needle: &str) -> Option<usize>;
}

impl ParseOffset for String {
    fn parse_offset(&self, needle: &str) -> Option<usize> {
        self.find(needle)
    }
}

// ------------------------------------------------------------- the fixtures

const RUST: &str = "\
pub fn scale(w: usize, h: usize) -> usize {
    w * 2 + h * 3
}

pub fn describe(a: bool, b: bool) -> usize {
    if a {
        1
    } else {
        2
    }
}

pub fn neither(a: bool, b: bool) -> bool {
    !(a && b)
}

pub fn report(ready: bool) {
    if ready {
        let first = 1;
        let second = first + 1;
        emit(second);
    }
}

pub fn emit(n: usize) {
    let _ = n;
}

pub fn callers() {
    emit(old_api(1));
    emit(old_api(2));
}

pub fn old_api(n: usize) -> usize {
    n
}
";

const GO: &str = "\
package gate

func Scale(w int, h int) int {
\treturn w*2 + h*3
}

func Describe(a bool, b bool) int {
\tif a {
\t\treturn 1
\t} else {
\t\treturn 2
\t}
}

func Neither(a bool, b bool) bool {
\treturn !(a && b)
}

func Report(ready bool) {
\tif ready {
\t\tfirst := 1
\t\tsecond := first + 1
\t\tEmit(second)
\t}
}

func Emit(n int) {
\t_ = n
}

func Callers() {
\tEmit(OldAPI(1))
\tEmit(OldAPI(2))
}

func OldAPI(n int) int {
\treturn n
}
";

const TYPESCRIPT: &str = "\
export function scale(w: number, h: number): number {
  return w * 2 + h * 3;
}

export function describe(a: boolean, b: boolean): number {
  if (a) {
    return 1;
  } else {
    return 2;
  }
}

export function neither(a: boolean, b: boolean): boolean {
  return !(a && b);
}

export function report(ready: boolean): void {
  if (ready) {
    const first = 1;
    const second = first + 1;
    emit(second);
  }
}

export function emit(n: number): void {
  void n;
}

export function callers(): void {
  emit(oldApi(1));
  emit(oldApi(2));
}

export function oldApi(n: number): number {
  return n;
}
";

const PYTHON: &str = "\
def scale(w, h):
    return w * 2 + h * 3


def describe(a, b):
    if a:
        return 1
    else:
        return 2


def neither(a, b):
    return not (a and b)


def report(ready):
    if ready:
        first = 1
        second = first + 1
        emit(second)


def emit(n):
    return n


def callers():
    emit(old_api(1))
    emit(old_api(2))


def old_api(n):
    return n


def check():
    assert scale(1, 2) == 8
    assert describe(True, False) == 1
    assert neither(True, True) is False
    report(True)
    callers()
";

const ZIG: &str = "\
pub fn scale(w: usize, h: usize) usize {
    return w * 2 + h * 3;
}

pub fn describe(a: bool, b: bool) usize {
    _ = b;
    if (a) {
        return 1;
    } else {
        return 2;
    }
}

pub fn neither(a: bool, b: bool) bool {
    return !(a and b);
}

pub fn report(ready: bool) void {
    if (ready) {
        const first: usize = 1;
        const second = first + 1;
        emit(second);
    }
}

pub fn emit(n: usize) void {
    _ = n;
}

pub fn callers() void {
    emit(oldApi(1));
    emit(oldApi(2));
}

pub fn oldApi(n: usize) usize {
    return n;
}

pub fn main() void {
    _ = scale(1, 2);
    _ = describe(true, false);
    _ = neither(true, true);
    report(true);
    callers();
}
";

const JAVA: &str = "\
public class Main {
    static int scale(int w, int h) {
        return w * 2 + h * 3;
    }

    static int describe(boolean a, boolean b) {
        if (a) {
            return 1;
        } else {
            return 2;
        }
    }

    static boolean neither(boolean a, boolean b) {
        return !(a && b);
    }

    static void report(boolean ready) {
        if (ready) {
            int first = 1;
            int second = first + 1;
            emit(second);
        }
    }

    static void emit(int n) {
        assert n >= 0;
    }

    static void callers() {
        emit(oldApi(1));
        emit(oldApi(2));
    }

    static int oldApi(int n) {
        return n;
    }

    public static void main(String[] args) {
        emit(scale(1, 2));
        emit(describe(true, false));
        callers();
    }
}
";

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            language: Language::Rust,
            toolchain: Toolchain::Cargo,
            file: "src/lib.rs",
            files: &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"gate-rewrites\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", RUST),
            ],
            expressions: &["w * 2 + h * 3", "a && b", "old_api(1)", "first + 1"],
            statements: Some(("let first = 1;", "emit(second);")),
            invert_if: Some("if a {"),
            de_morgan: Some("!(a && b)"),
            guard_clause: Some("if ready {"),
            restructures: &[
                ("old_api($X)", "old_api($X + 0)", "old_api(1 + 0)"),
                ("emit($X)", "emit($X + 0)", "emit(old_api(1) + 0)"),
                ("$A * 2 + $B * 3", "$B * 3 + $A * 2", "h * 3 + w * 2"),
            ],
        },
        Fixture {
            language: Language::Go,
            toolchain: Toolchain::Go,
            file: "gate.go",
            files: &[("go.mod", "module gate\n\ngo 1.21\n"), ("gate.go", GO)],
            expressions: &["w*2 + h*3", "a && b", "OldAPI(1)", "first + 1"],
            statements: Some(("first := 1", "Emit(second)")),
            invert_if: Some("if a {"),
            de_morgan: Some("!(a && b)"),
            guard_clause: Some("if ready {"),
            restructures: &[
                ("OldAPI($X)", "OldAPI($X + 0)", "OldAPI(1 + 0)"),
                ("Emit($X)", "Emit($X + 0)", "Emit(OldAPI(1) + 0)"),
            ],
        },
        Fixture {
            language: Language::TypeScript,
            toolchain: Toolchain::Tsc,
            file: "src/main.ts",
            files: &[
                (
                    "tsconfig.json",
                    "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  },\n  \"include\": [\"src\"]\n}\n",
                ),
                ("src/main.ts", TYPESCRIPT),
            ],
            expressions: &["w * 2 + h * 3", "a && b", "oldApi(1)", "first + 1"],
            statements: Some(("const first = 1;", "emit(second);")),
            invert_if: Some("if (a) {"),
            de_morgan: Some("!(a && b)"),
            guard_clause: Some("if (ready) {"),
            restructures: &[
                ("oldApi($X)", "oldApi($X + 0)", "oldApi(1 + 0)"),
                ("emit($X)", "emit($X + 0)", "emit(oldApi(1) + 0)"),
                ("$A * 2 + $B * 3", "$B * 3 + $A * 2", "h * 3 + w * 2"),
            ],
        },
        Fixture {
            language: Language::Python,
            toolchain: Toolchain::Python,
            file: "main.py",
            files: &[("main.py", PYTHON)],
            expressions: &["w * 2 + h * 3", "a and b", "old_api(1)", "first + 1"],
            statements: Some(("first = 1", "emit(second)")),
            invert_if: Some("if a:"),
            de_morgan: Some("not (a and b)"),
            guard_clause: Some("if ready:"),
            restructures: &[
                ("old_api($X)", "old_api($X + 0)", "old_api(1 + 0)"),
                ("emit($X)", "emit($X + 0)", "emit(old_api(1) + 0)"),
            ],
        },
        Fixture {
            language: Language::Zig,
            toolchain: Toolchain::Zig,
            file: "main.zig",
            files: &[("main.zig", ZIG)],
            expressions: &["w * 2 + h * 3", "a and b", "oldApi(1)", "first + 1"],
            statements: Some(("const first: usize = 1;", "emit(second);")),
            invert_if: Some("if (a) {"),
            de_morgan: Some("!(a and b)"),
            guard_clause: Some("if (ready) {"),
            restructures: &[
                ("oldApi($X)", "oldApi($X + 0)", "oldApi(1 + 0)"),
                ("emit($X)", "emit($X + 0)", "emit(oldApi(1) + 0)"),
            ],
        },
        Fixture {
            language: Language::Java,
            toolchain: Toolchain::Javac,
            file: "Main.java",
            files: &[("Main.java", JAVA)],
            expressions: &["w * 2 + h * 3", "a && b", "oldApi(1)", "first + 1"],
            // Java claims no extract at all, which is a claim this checks by asking.
            statements: None,
            invert_if: Some("if (a) {"),
            de_morgan: Some("!(a && b)"),
            guard_clause: Some("if (ready) {"),
            restructures: &[
                ("oldApi($X)", "oldApi($X + 0)", "oldApi(1 + 0)"),
                ("emit($X)", "emit($X + 0)", "emit(oldApi(1) + 0)"),
            ],
        },
    ]
}

fn skip(fixture: &Fixture) -> bool {
    if fixture.toolchain.is_available() {
        return false;
    }
    eprintln!(
        "rewrite gate: {} skipped, {} is not on PATH",
        fixture.language,
        fixture.toolchain.program()
    );
    true
}

// ------------------------------------------------------------------ the sweep

#[test]
fn every_fixture_compiles_before_anything_touches_it() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let ws = fixture.workspace();
        if let Err(e) = ws.compiles() {
            panic!(
                "the {} fixture is broken to begin with:\n{e}",
                fixture.language
            );
        }
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("the fixtures as written", &[]);
}

#[test]
fn extracting_an_expression_compiles_in_every_language() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        for expression in fixture.expressions {
            let ws = fixture.workspace();
            let span = fixture.span_of(&ws, expression);
            let index = ws.index();
            let planned = fun_refactor::refactor::extract::variable(
                &index,
                &ws.path(fixture.file),
                span,
                "lifted",
                false,
            )
            .map(|p| p.edits);
            let compiled = gate(
                &format!("extracting `{expression}` in {}", fixture.language),
                &ws,
                planned,
            );
            run.record(fixture.language.name(), compiled);
        }
    }
    // Java has no binding form the matrix claims, and every expression in it refused
    // while this test reported success.
    run.expect_refusals("extract variable", &["java"]);
}

#[test]
fn extracting_a_function_compiles_in_every_language() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some((first, last)) = fixture.statements else {
            // No statements to lift is a gap in the fixture, not a result about the
            // language, so it counts as skipped rather than as a language that worked.
            run.skip(fixture.language.name());
            continue;
        };
        let ws = fixture.workspace();
        let start = fixture.span_of(&ws, first).start;
        let end = fixture.span_of(&ws, last).end;
        let index = ws.index();
        let planned = fun_refactor::refactor::extract::function(
            &index,
            &ws.path(fixture.file),
            Span::new(start, end),
            "announce",
        )
        .map(|p| p.edits);
        let compiled = gate(
            &format!("extracting a function in {}", fixture.language),
            &ws,
            planned,
        );
        run.record(fixture.language.name(), compiled);
    }
    run.expect_refusals("extract function", &[]);
}

#[test]
fn every_rewrite_compiles_in_every_language() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        for (rewrite, needle) in [
            (Rewrite::InvertIf, fixture.invert_if),
            (Rewrite::DeMorgan, fixture.de_morgan),
            (Rewrite::GuardClause, fixture.guard_clause),
        ] {
            // A fixture without a construct for this rewrite says nothing about the
            // language, so it is a skip and not a pass.
            let Some(needle) = needle else {
                continue;
            };
            let ws = fixture.workspace();
            let at = fixture.span_of(&ws, needle).start;
            let index = ws.index();
            let planned =
                fun_refactor::refactor::rewrite::apply(&index, &ws.path(fixture.file), at, rewrite)
                    .map(|p| p.edits);
            let compiled = gate(
                &format!("{} in {}", rewrite.as_str(), fixture.language),
                &ws,
                planned,
            );
            run.record(fixture.language.name(), compiled);
        }
    }
    run.expect_refusals("rewrites", &[]);
}

#[test]
fn restructuring_every_occurrence_compiles_in_every_language() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        for (pattern, template, expected) in fixture.restructures {
            let ws = fixture.workspace();
            let index = ws.index();
            let planned = fun_refactor::refactor::restructure::apply(
                &index,
                fixture.language,
                pattern,
                template,
            )
            .map(|p| p.edits);
            must_plan(
                &format!("restructuring `{pattern}` in {}", fixture.language),
                &ws,
                planned,
            );
            let after = ws.read(fixture.file);
            assert!(
                after.contains(expected),
                "`{pattern}` in {} had to produce `{expected}`:\n{after}",
                fixture.language
            );
        }
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("restructure", &[]);
}

#[test]
fn inverting_an_if_twice_returns_it_to_what_it_was() {
    let mut run = GateRun::default();
    // The strongest thing that can be asked of a rewrite without running the program: it
    // has an inverse, and applying both leaves the source where it started. A rewrite that
    // dropped an `else`, reordered a branch's statements or left a stray `!!` would
    // compile and would not survive this.
    //
    // `de-morgan` is not asked the same question and is right not to be: it pushes a
    // negation into a conjunction, and the result carries no negated conjunction to push
    // back out. The command says so rather than doing something.
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some(needle) = fixture.invert_if else {
            continue;
        };
        let ws = fixture.workspace();
        let before = ws.read(fixture.file);

        for round in 1..=2 {
            let at = fixture
                .span_of(&ws, needle_now(&ws, fixture.file, needle, round))
                .start;
            let index = ws.index();
            let planned = fun_refactor::refactor::rewrite::apply(
                &index,
                &ws.path(fixture.file),
                at,
                Rewrite::InvertIf,
            );
            match planned {
                Ok(plan) => ws.apply(&plan.edits),
                Err(e) => panic!(
                    "{} could not invert on round {round}: {e}",
                    fixture.language
                ),
            }
        }

        assert_eq!(
            ws.read(fixture.file),
            before,
            "inverting twice in {} did not return the source to what it was",
            fixture.language
        );
        if let Err(e) = ws.compiles() {
            panic!(
                "{} does not compile after two inversions:\n{e}",
                fixture.language
            );
        }
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("inverting an if twice", &[]);
}

/// The `if` to invert on this round.
///
/// After the first inversion the condition carries a negation, so the text that found it
/// the first time is no longer there.
fn needle_now(ws: &Workspace, file: &str, original: &'static str, round: usize) -> &'static str {
    if round == 1 {
        return original;
    }
    let negated: &[(&str, &str)] = &[
        ("if a {", "if !a {"),
        ("if (a) {", "if (!a) {"),
        ("if a:", "if not a:"),
    ];
    let source = ws.read(file);
    for (plain, inverted) in negated {
        if *plain == original && source.contains(inverted) {
            return inverted;
        }
    }
    panic!("nothing in {file} looks like an inverted `{original}`:\n{source}");
}

#[test]
fn a_language_the_matrix_marks_unavailable_refuses_by_name() {
    // Java is the one language here that claims neither kind of extraction. A claim that
    // nothing checks is a claim that drifts, so this asks for both and reads the answer.
    let fixture = fixtures()
        .into_iter()
        .find(|f| f.language == Language::Java)
        .expect("the Java fixture");
    if skip(&fixture) {
        return;
    }
    let ws = fixture.workspace();
    let index = ws.index();
    let span = fixture.span_of(&ws, fixture.expressions[0]);

    let variable = fun_refactor::refactor::extract::variable(
        &index,
        &ws.path(fixture.file),
        span,
        "scaled",
        false,
    )
    .expect_err("the matrix marks extract variable unavailable for java");
    assert!(
        variable.to_string().contains("java"),
        "the refusal names the language: {variable}"
    );

    let function =
        fun_refactor::refactor::extract::function(&index, &ws.path(fixture.file), span, "announce")
            .expect_err("the matrix marks extract function unavailable for java");
    assert!(
        function.to_string().contains("java"),
        "the refusal names the language: {function}"
    );
}

/// Every position in the fixture where any rewrite applies.
///
/// The tests above name a position each. This asks the command itself where it would act,
/// which is the difference between checking the case somebody thought of and checking the
/// cases the fixture actually contains.
fn every_applicable_position(ws: &Workspace, file: &str) -> Vec<(usize, Rewrite)> {
    let source = ws.read(file);
    let index = ws.index();
    let path = ws.path(file);

    // One entry per shape, and a shape is a line: every offset inside an `if` reports the
    // same rewrite, and applying it to each of them checks it once and pays for it fifty
    // times.
    let mut found: Vec<(usize, Rewrite)> = Vec::new();
    let mut seen: Vec<(usize, Rewrite)> = Vec::new();
    let line_of = |at: usize| source[..at].matches('\n').count();

    for (at, _) in source.char_indices() {
        let Ok(available) = fun_refactor::refactor::rewrite::available(&index, &path, at) else {
            continue;
        };
        for rewrite in available {
            let key = (line_of(at), rewrite);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            found.push((at, rewrite));
        }
    }
    found
}

#[test]
fn every_rewrite_the_command_offers_compiles() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let probe = fixture.workspace();
        let positions = every_applicable_position(&probe, fixture.file);
        assert!(
            positions.len() >= 3,
            "{} offered only {} rewrites, so this swept almost nothing",
            fixture.language,
            positions.len()
        );

        for (at, rewrite) in &positions {
            let ws = fixture.workspace();
            let index = ws.index();
            let planned = fun_refactor::refactor::rewrite::apply(
                &index,
                &ws.path(fixture.file),
                *at,
                *rewrite,
            )
            .map(|p| p.edits);
            let compiled = gate(
                &format!(
                    "{} at offset {at} in {}",
                    rewrite.as_str(),
                    fixture.language
                ),
                &ws,
                planned,
            );
            run.record(fixture.language.name(), compiled);
        }
        eprintln!(
            "rewrite sweep: {} — {} position(s) offered and checked",
            fixture.language,
            positions.len()
        );
    }
    // Every position the command offers has to survive its own compiler: an offer that
    // refuses is the command contradicting itself.
    run.expect_refusals("the rewrites the command offers", &[]);
}

/// What this file covers, said out loud.
#[test]
fn the_rewrite_gate_states_what_it_covers() {
    let mut missing = Vec::new();
    for fixture in fixtures() {
        if !fixture.toolchain.is_available() {
            missing.push(format!(
                "{} ({})",
                fixture.language,
                fixture.toolchain.program()
            ));
        }
        eprintln!(
            "rewrite gate: {} — {} ({})",
            fixture.language,
            fixture.toolchain.covers(),
            match fixture.toolchain.is_available() {
                true => "ran here",
                false => "skipped, its toolchain is absent here",
            }
        );
    }
    common::require_on_ci("rewrite gate", &missing);
    eprintln!(
        "rewrite gate: not driven — tsx, bash, html, css, scss, hcl, yaml, helm, xml, markdown"
    );
}
