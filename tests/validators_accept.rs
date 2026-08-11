//! Does the code a refactoring writes still satisfy the tool that owns the language?
//!
//! The compile gate covered six languages because "has a compiler" was the bar. That was
//! the wrong bar. `terraform validate` resolves references, `helm lint` renders the chart
//! and checks it against Kubernetes' schemas, `bash -n` runs the shell's own parser, and
//! `xmllint` decides well-formedness. Each of them rejects things tree-sitter reads
//! happily — which is the exact gap that produced every defect the other three gate files
//! found.
//!
//! Five languages here, and each drives every command the capability matrix claims for it.
//! A refusal passes; writing something the language's own tool then rejects does not.
//!
//! Not driven, and why: **scss** has no `sass` on this machine, **markdown** has nothing to
//! validate, and **yaml** is checked as part of the chart `helm lint` renders.

mod common;
use common::{gate, must_plan, GateRun, Toolchain, Workspace};

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolId;
use fun_refactor::span::Span;

/// One language's fixture, and the shapes each command needs from it.
struct Fixture {
    language: Language,
    toolchain: Toolchain,
    file: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The declaration to rename, and what to call it.
    rename: (&'static str, &'static str),
    /// A declaration nothing uses, which `fr delete` may take.
    doomed: Option<&'static str>,
    /// An expression to lift into a binding, where the language has bindings.
    expression: Option<&'static str>,
    /// A shape that occurs more than once, what it becomes, and what must then appear.
    ///
    /// In bash, `$NAME` in a pattern is a metavariable and not a shell expansion — the
    /// language and the pattern syntax spell the same thing the same way. `$1` is literal
    /// because a metavariable must start with a letter, and a named expansion is written
    /// `$$name`. Getting that wrong writes `greet "${target}"` over `greet "literal"`,
    /// which `bash -n` and shellcheck both accept and only running the script rejects.
    restructure: Option<(&'static str, &'static str, &'static str)>,
    /// A declaration to inline, where the language has bindings.
    inline: Option<&'static str>,
    /// A callable whose first two parameters may swap.
    signature: Option<&'static str>,
    /// A flag to remove, and the destination file for a move.
    flag: Option<&'static str>,
    moves: Option<(&'static str, &'static str, &'static str)>,
}

impl Fixture {
    fn workspace(&self) -> Workspace {
        Workspace::with(self.toolchain, self.files)
    }

    fn span_of(&self, ws: &Workspace, needle: &str) -> Span {
        let source = ws.read(self.file);
        let at = source
            .find(needle)
            .unwrap_or_else(|| panic!("{} does not contain {needle:?}", self.file));
        Span::new(at, at + needle.len())
    }
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

fn skip(fixture: &Fixture) -> bool {
    if fixture.toolchain.is_available() {
        return false;
    }
    eprintln!(
        "validator gate: {} skipped, {} is not on PATH",
        fixture.language,
        fixture.toolchain.program()
    );
    true
}

// ------------------------------------------------------------- the fixtures

const BASH: &str = "\
#!/usr/bin/env bash
set -euo pipefail

DEFAULT_NAME=\"world\"

greet() {
  echo \"hello $1 and $2\"
}

unused_helper() {
  echo \"nobody calls this\"
}

main() {
  local target
  target=\"${1:-$DEFAULT_NAME}\"
  greet \"$target\" \"friend\"
}

check() {
  local got
  got=\"$(greet \"a\" \"b\")\"
  [ \"$got\" = \"hello a and b\" ] || { echo \"greet said: $got\" >&2; exit 1; }
}

main \"$@\"
check
";

const HCL: &str = "\
variable \"enabled\" {
  type    = bool
  default = true
}

variable \"unused_input\" {
  type    = string
  default = \"nothing reads this\"
}

locals {
  base_name = \"service\"
  full_name = \"${local.base_name}-primary\"
  scaled    = var.enabled ? 2 : 1
}

output \"scaled\" {
  value = local.scaled
}

output \"primary\" {
  value = local.full_name
}

output \"secondary\" {
  value = local.base_name
}
";

const HELM_TEMPLATE: &str = "\
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .Values.name }}
spec:
  replicas: {{ .Values.replicas }}
  selector:
    matchLabels:
      app: {{ .Values.name }}
  template:
    metadata:
      labels:
        app: {{ .Values.name }}
    spec:
      containers:
        - name: main
          image: \"nginx:{{ .Values.image.tag }}\"
";

/// XML's own binding forms, which are narrower than they look: an `id` attribute, an
/// `xmlns:` prefix, and a DTD entity. A `name="timeout"` attribute declares nothing —
/// only a DTD could say that it does, and reading one is a gap this records rather than
/// guesses at.
const XML: &str = "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE configuration [
  <!ENTITY brand \"Acme\">
  <!ENTITY unusedEntity \"nobody expands this\">
]>
<configuration>
  <section id=\"limits\">
    <setting>30</setting>
  </section>
  <section id=\"unusedSection\">
    <setting>3</setting>
  </section>
  <link href=\"#limits\">see &brand;</link>
</configuration>
";

/// HTML declares element ids and refers to them; a `class` is a *reference* to a
/// selector some stylesheet declares, so a fixture with no stylesheet has nothing to
/// rename there.
const HTML: &str = "\
<!DOCTYPE html>
<html>
  <head>
    <title>Gate</title>
  </head>
  <body>
    <p id=\"intro\">first</p>
    <p id=\"unusedAnchor\">second</p>
    <a href=\"#intro\">back to the top</a>
  </body>
</html>
";

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            language: Language::Bash,
            toolchain: Toolchain::Bash,
            file: "run.sh",
            files: &[("run.sh", BASH)],
            rename: ("greet", "salute"),
            doomed: Some("unused_helper"),
            expression: Some("\"${1:-$DEFAULT_NAME}\""),
            restructure: Some((
                "echo \"hello $1 and $2\"",
                "printf 'hello %s and %s\\n' \"$1\" \"$2\"",
                "printf",
            )),
            inline: Some("DEFAULT_NAME"),
            signature: Some("greet"),
            flag: None,
            moves: Some((
                "greet",
                "lib.sh",
                "#!/usr/bin/env bash\nother() {\n  :\n}\n",
            )),
        },
        Fixture {
            language: Language::Hcl,
            toolchain: Toolchain::Terraform,
            file: "main.tf",
            files: &[("main.tf", HCL)],
            rename: ("base_name", "stem"),
            doomed: Some("unused_input"),
            expression: None,
            restructure: None,
            inline: Some("base_name"),
            signature: None,
            flag: Some("enabled"),
            moves: Some(("full_name", "other.tf", "locals {\n  spare = 1\n}\n")),
        },
        Fixture {
            language: Language::Helm,
            toolchain: Toolchain::Helm,
            file: "templates/deployment.yaml",
            files: &[
                ("Chart.yaml", "apiVersion: v2\nname: gate\nversion: 0.1.0\n"),
                (
                    "values.yaml",
                    "name: gate\nreplicas: 2\nimage:\n  tag: v1\nunused_key: nothing\n",
                ),
                ("templates/deployment.yaml", HELM_TEMPLATE),
            ],
            rename: ("replicas", "replicaCount"),
            doomed: None,
            expression: None,
            restructure: None,
            inline: None,
            signature: None,
            flag: None,
            moves: None,
        },
        Fixture {
            language: Language::Xml,
            toolchain: Toolchain::Xmllint,
            file: "config.xml",
            files: &[("config.xml", XML)],
            rename: ("limits", "bounds"),
            doomed: Some("unusedEntity"),
            expression: None,
            restructure: None,
            inline: Some("brand"),
            signature: None,
            flag: None,
            moves: None,
        },
        Fixture {
            language: Language::Html,
            toolchain: Toolchain::XmllintHtml,
            file: "page.html",
            files: &[("page.html", HTML)],
            rename: ("intro", "opening"),
            doomed: None,
            expression: None,
            restructure: None,
            inline: None,
            signature: None,
            flag: None,
            moves: None,
        },
    ]
}

// ------------------------------------------------------------------ the sweep

#[test]
fn every_fixture_satisfies_its_validator_before_anything_touches_it() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let ws = fixture.workspace();
        if let Err(e) = ws.compiles() {
            panic!(
                "the {} fixture is rejected before any refactoring:\n{e}",
                fixture.language
            );
        }
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("the fixtures as written", &[]);
}

#[test]
fn renaming_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let (from, to) = fixture.rename;
        let ws = fixture.workspace();
        let index = ws.index();
        let planned =
            fun_refactor::refactor::rename::plan(&index, symbol(&index, from), to).map(|p| p.edits);
        must_plan(
            &format!("renaming `{from}` in {}", fixture.language),
            &ws,
            planned,
        );
        let after = ws.read(fixture.file);
        assert!(
            after.contains(to),
            "{} did not write the new name:\n{after}",
            fixture.language
        );
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("rename", &[]);
}

#[test]
fn deleting_something_nothing_uses_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some(doomed) = fixture.doomed else {
            continue;
        };
        let ws = fixture.workspace();
        let index = ws.index();
        let planned =
            fun_refactor::refactor::delete::plan(&index, symbol(&index, doomed)).map(|p| p.edits);
        must_plan(
            &format!("deleting `{doomed}` in {}", fixture.language),
            &ws,
            planned,
        );
        assert!(
            !ws.read(fixture.file).contains(doomed),
            "{} kept what it said it deleted",
            fixture.language
        );
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("delete", &[]);
}

#[test]
fn extracting_a_binding_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some(expression) = fixture.expression else {
            continue;
        };
        let ws = fixture.workspace();
        let span = fixture.span_of(&ws, expression);
        let index = ws.index();
        let planned = fun_refactor::refactor::extract::variable(
            &index,
            &ws.path(fixture.file),
            span,
            "chosen",
            false,
        )
        .map(|p| p.edits);
        let compiled = gate(
            &format!("extracting a binding in {}", fixture.language),
            &ws,
            planned,
        );
        run.record(fixture.language.name(), compiled);
    }
    run.expect_refusals("extract a binding", &[]);
}

#[test]
fn restructuring_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some((pattern, template, expected)) = fixture.restructure else {
            continue;
        };
        let ws = fixture.workspace();
        let index = ws.index();
        let planned =
            fun_refactor::refactor::restructure::apply(&index, fixture.language, pattern, template)
                .map(|p| p.edits);
        must_plan(
            &format!("restructuring in {}", fixture.language),
            &ws,
            planned,
        );
        let after = ws.read(fixture.file);
        assert!(
            after.contains(expected),
            "{} did not produce `{expected}`:\n{after}",
            fixture.language
        );
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("restructure", &[]);
}

#[test]
fn inlining_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some(name) = fixture.inline else {
            continue;
        };
        let ws = fixture.workspace();
        let index = ws.index();
        let planned =
            fun_refactor::refactor::inline::variable(&index, symbol(&index, name)).map(|p| p.edits);
        let compiled = gate(
            &format!("inlining `{name}` in {}", fixture.language),
            &ws,
            planned,
        );
        run.record(fixture.language.name(), compiled);
    }
    run.expect_refusals("inline a binding", &[]);
}

#[test]
fn changing_a_signature_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some(name) = fixture.signature else {
            continue;
        };
        let ws = fixture.workspace();
        let index = ws.index();
        let planned = fun_refactor::refactor::signature::change(
            &index,
            symbol(&index, name),
            fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
        )
        .map(|p| p.edits);
        let compiled = gate(
            &format!("changing `{name}`'s signature in {}", fixture.language),
            &ws,
            planned,
        );
        run.record(fixture.language.name(), compiled);
    }
    run.expect_refusals("change a signature", &[]);
}

#[test]
fn removing_a_flag_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some(flag) = fixture.flag else {
            continue;
        };
        let ws = fixture.workspace();
        let planned = fun_refactor::refactor::cascade::remove_flag_in(ws.sources(), flag, true)
            .map(|p| p.edits);
        let compiled = gate(
            &format!("removing `{flag}` in {}", fixture.language),
            &ws,
            planned,
        );
        run.record(fixture.language.name(), compiled);
    }
    run.expect_refusals("remove a flag", &[]);
}

#[test]
fn moving_a_declaration_keeps_the_validator_happy() {
    let mut run = GateRun::default();
    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        let Some((name, destination, seed)) = fixture.moves else {
            continue;
        };
        let ws = fixture.workspace();
        std::fs::write(ws.path(destination), seed).expect("write");
        let index = ws.index();
        let planned = fun_refactor::refactor::move_symbol::to_file(
            &index,
            symbol(&index, name),
            &ws.path(destination),
        )
        .map(|p| p.edits);
        let compiled = gate(
            &format!("moving `{name}` in {}", fixture.language),
            &ws,
            planned,
        );
        run.record(fixture.language.name(), compiled);
    }
    run.expect_refusals("move a symbol", &[]);
}

/// The gate has to be able to fail, for every validator here.
///
/// Each of these passes when the tool behaves, and would also pass if the validator
/// stopped running — a wrong path, a flag that means nothing, an exit code nobody reads.
/// This breaks each fixture on purpose and checks the validator says so.
#[test]
fn every_validator_reports_a_workspace_it_should_reject() {
    let mut run = GateRun::default();
    let broken: &[(Language, &str, &str)] = &[
        (
            Language::Bash,
            "run.sh",
            "#!/usr/bin/env bash\ngreet() {\n  echo \"unclosed\n",
        ),
        (
            Language::Hcl,
            "main.tf",
            "output \"primary\" {\n  value = local.nothing_declares_this\n}\n",
        ),
        (
            Language::Helm,
            "templates/deployment.yaml",
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: {{ .Values.nosuch.deep }}\n",
        ),
        (
            Language::Xml,
            "config.xml",
            "<?xml version=\"1.0\"?>\n<root>\n  <child>text</wrong>\n</root>\n",
        ),
        (
            Language::Html,
            "page.html",
            "<!DOCTYPE html>\n<html><body><p>hi</div></body></html>\n",
        ),
    ];

    for fixture in fixtures() {
        if skip(&fixture) {
            run.skip(fixture.language.name());
            continue;
        }
        // No broken counterpart written for this language yet, which is a gap in the
        // table above rather than a fact about the validator.
        let Some((_, file, content)) = broken.iter().find(|(l, _, _)| *l == fixture.language)
        else {
            run.skip(fixture.language.name());
            continue;
        };
        let ws = fixture.workspace();
        std::fs::write(ws.path(file), content).expect("write");
        assert!(
            ws.compiles().is_err(),
            "{}'s validator accepted a file it should have rejected",
            fixture.language
        );
        run.record(fixture.language.name(), true);
    }
    run.expect_refusals("a workspace every validator should reject", &[]);
}

/// What this file covers, said out loud.
#[test]
fn the_validator_gate_states_what_it_covers() {
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
            "validator gate: {} — {} ({})",
            fixture.language,
            fixture.toolchain.covers(),
            match fixture.toolchain.is_available() {
                true => "ran here",
                false => "skipped, its tool is absent here",
            }
        );
    }
    common::require_on_ci("validator gate", &missing);
    eprintln!(
        "validator gate: not driven — scss (no sass here), markdown (nothing to validate), \
         yaml (checked as part of the chart helm lint renders)"
    );
}
