//! Change-signature for the config languages: Terraform module variables, and the
//! SCSS mixin cell that the grammar makes impossible.
//!
//! A Terraform module is a directory. Its parameters are the `variable "x" {}` blocks
//! declared there and its call sites are `module "m" { source = "./that/dir" }` blocks
//! elsewhere, so a signature change has to reach both sides at once. These tests assert
//! the exact resulting bytes, because "formatting outside the edited range survives" is
//! only checkable against exact text.

use fun_refactor::{
    edit::{apply_to_string, plan, Validation},
    index::Index,
    lang::Language,
    model::SymbolKind,
    parse::Parsers,
    refactor::{
        signature::{self, Change, Subject},
        Refusal,
    },
    scan::{scan, ScanOptions},
};
use std::path::{Path, PathBuf};

// --------------------------------------------------------------- harness

struct Workspace {
    dir: tempfile::TempDir,
    index: Index,
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        let scanned = scan(dir.path(), &ScanOptions::default()).unwrap();
        let index = Index::build_from_scan(&scanned).unwrap();
        Workspace { dir, index }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The one symbol with this name, of this kind.
    fn symbol(&self, name: &str, kind: SymbolKind) -> fun_refactor::model::SymbolId {
        let matches: Vec<_> = self
            .index
            .find_symbols(name, None)
            .into_iter()
            .filter(|s| s.kind == kind)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one {kind:?} named {name}, got {matches:?}"
        );
        matches[0].id
    }
}

fn applied(plan: &signature::SignaturePlan, path: &Path) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    match plan.edits.edits_for(path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

/// A typed refusal, not a generic failure: the caller has to be able to tell the
/// two apart.
fn refusal(error: anyhow::Error) -> String {
    assert!(
        error.downcast_ref::<Refusal>().is_some(),
        "expected a typed Refusal, got: {error}"
    );
    error.to_string()
}

// A module with two variables, one required (and used inside the module) and one
// optional (and unused), called once from the root module.
const ROOT_MAIN_TF: &str = r#"terraform {
  required_version = ">= 1.5"
}

module "thing" {
  source = "./modules/thing"
  region = "eu-west-1"
  size   = 3
}
"#;

const THING_VARIABLES_TF: &str = r#"variable "region" {
  type = string
}

variable "size" {
  type    = number
  default = 1
}
"#;

const THING_MAIN_TF: &str = r#"resource "null_resource" "r" {
  triggers = {
    region = var.region
  }
}
"#;

fn realistic() -> Workspace {
    Workspace::new(&[
        ("main.tf", ROOT_MAIN_TF),
        ("modules/thing/variables.tf", THING_VARIABLES_TF),
        ("modules/thing/main.tf", THING_MAIN_TF),
    ])
}

// ------------------------------------------------------ the shape it acts on

#[test]
fn the_module_call_surface_is_indexed_as_expected() {
    // The rest of the tests lean on these facts; if the extraction changes shape,
    // this is the test that should fail first.
    let ws = realistic();
    let region = ws.symbol("region", SymbolKind::Variable);
    assert_eq!(
        ws.index.symbol(region).unwrap().file,
        ws.path("modules/thing/variables.tf")
    );

    // `module "thing"` is a Module symbol, and its source is recorded as an import.
    let thing = ws
        .index
        .symbol(ws.symbol("thing", SymbolKind::Module))
        .unwrap();
    assert_eq!(thing.file, ws.path("main.tf"));
    let imports = &ws.index.file(&ws.path("main.tf")).unwrap().imports;
    assert_eq!(imports.len(), 1, "got {imports:?}");
    assert_eq!(imports[0].path, "./modules/thing");
    assert_eq!(imports[0].alias.as_deref(), Some("thing"));

    // The module's own `var.region` read resolves back to the declaration.
    assert_eq!(ws.index.references_to(region).len(), 1);
    assert!(ws
        .index
        .references_to(ws.symbol("size", SymbolKind::Variable))
        .is_empty());
}

// --------------------------------------------------------------- removing

#[test]
fn removing_a_variable_deletes_the_block_and_every_argument() {
    let ws = realistic();
    // Document order across the module directory: 0 = region, 1 = size.
    let size = ws.symbol("size", SymbolKind::Variable);
    let plan = signature::change(&ws.index, size, Change::Remove(1)).unwrap();

    assert_eq!(plan.subject_kind, Subject::TerraformModule);
    assert_eq!(plan.call_sites, 1);
    assert_eq!(
        applied(&plan, &ws.path("modules/thing/variables.tf")),
        "variable \"region\" {\n  type = string\n}\n"
    );
    assert_eq!(
        applied(&plan, &ws.path("main.tf")),
        "terraform {\n  required_version = \">= 1.5\"\n}\n\n\
         module \"thing\" {\n  source = \"./modules/thing\"\n  region = \"eu-west-1\"\n}\n"
    );
    // The module's own configuration is untouched.
    assert_eq!(
        applied(&plan, &ws.path("modules/thing/main.tf")),
        THING_MAIN_TF
    );
}

#[test]
fn a_removal_reparses_clean_in_both_files() {
    let ws = realistic();
    let size = ws.symbol("size", SymbolKind::Variable);
    let change = signature::change(&ws.index, size, Change::Remove(1)).unwrap();
    let outcomes = plan(&change.edits, Validation::ReparseStrict).unwrap();
    assert_eq!(outcomes.len(), 2, "declaration and call site");
    assert!(outcomes.iter().all(|o| o.changed()));
}

#[test]
fn removing_a_variable_the_module_still_reads_refuses() {
    // `var.region` inside the module would dangle, so the whole change is off.
    let ws = realistic();
    let region = ws.symbol("region", SymbolKind::Variable);
    let error = signature::change(&ws.index, region, Change::Remove(0))
        .unwrap_err()
        .to_string();
    assert!(error.contains("still read 1 time(s)"), "got: {error}");
    assert!(error.contains("var.region"), "got: {error}");
    assert!(error.contains("modules/thing/main.tf:3"), "got: {error}");
}

#[test]
fn a_caller_that_omitted_a_defaulted_argument_needs_no_edit() {
    // Nothing to remove there, and the result is still valid Terraform, so this is
    // not a partial update — it is a complete one that happens to touch one file.
    let ws = Workspace::new(&[
        (
            "main.tf",
            "module \"thing\" {\n  source = \"./modules/thing\"\n}\n",
        ),
        (
            "modules/thing/variables.tf",
            "variable \"size\" {\n  default = 1\n}\n",
        ),
    ]);
    let size = ws.symbol("size", SymbolKind::Variable);
    let plan = signature::change(&ws.index, size, Change::Remove(0)).unwrap();
    assert_eq!(plan.call_sites, 0);
    assert_eq!(applied(&plan, &ws.path("modules/thing/variables.tf")), "");
    assert_eq!(
        applied(&plan, &ws.path("main.tf")),
        "module \"thing\" {\n  source = \"./modules/thing\"\n}\n"
    );
}

#[test]
fn every_caller_in_the_workspace_is_updated() {
    let ws = Workspace::new(&[
        (
            "prod/main.tf",
            "module \"thing\" {\n  source = \"../modules/thing\"\n  size   = 3\n}\n",
        ),
        (
            "staging/main.tf",
            "module \"thing\" {\n  source = \"../modules/thing\"\n  size   = 1\n}\n",
        ),
        (
            "modules/thing/variables.tf",
            "variable \"size\" {\n  default = 1\n}\n",
        ),
    ]);
    let size = ws.symbol("size", SymbolKind::Variable);
    let plan = signature::change(&ws.index, size, Change::Remove(0)).unwrap();
    assert_eq!(plan.call_sites, 2);
    for env in ["prod", "staging"] {
        assert_eq!(
            applied(&plan, &ws.path(&format!("{env}/main.tf"))),
            "module \"thing\" {\n  source = \"../modules/thing\"\n}\n",
            "{env}"
        );
    }
}

#[test]
fn a_values_file_assignment_goes_with_the_variable() {
    // A `.tfvars` entry names the same call surface from the other side; left
    // behind it would set a variable that no longer exists.
    let ws = Workspace::new(&[
        ("variables.tf", "variable \"size\" {\n  default = 1\n}\n"),
        ("terraform.tfvars", "size   = 4\nregion = \"eu\"\n"),
    ]);
    let size = ws.symbol("size", SymbolKind::Variable);
    let plan = signature::change(&ws.index, size, Change::Remove(0)).unwrap();
    assert_eq!(applied(&plan, &ws.path("variables.tf")), "");
    assert_eq!(
        applied(&plan, &ws.path("terraform.tfvars")),
        "region = \"eu\"\n"
    );
}

#[test]
fn a_position_past_the_end_names_what_there_is() {
    let ws = realistic();
    let size = ws.symbol("size", SymbolKind::Variable);
    let error = signature::change(&ws.index, size, Change::Remove(7))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no module variable at position 7"),
        "got: {error}"
    );
    // Required-ness is what a caller most needs to know before touching the list.
    assert!(
        error.contains("0: region (required), 1: size"),
        "got: {error}"
    );
}

// ----------------------------------------------------------------- adding

#[test]
fn adding_a_variable_declares_it_and_passes_it_everywhere() {
    let ws = realistic();
    let thing = ws.symbol("thing", SymbolKind::Module);
    let plan = signature::change(
        &ws.index,
        thing,
        Change::Add {
            at: 2,
            declaration: "variable \"tags\" {\n  type = map(string)\n}".into(),
            argument: "{}".into(),
        },
    )
    .unwrap();

    assert_eq!(plan.call_sites, 1);
    assert_eq!(
        applied(&plan, &ws.path("modules/thing/variables.tf")),
        "variable \"region\" {\n  type = string\n}\n\n\
         variable \"size\" {\n  type    = number\n  default = 1\n}\n\n\
         variable \"tags\" {\n  type = map(string)\n}\n"
    );
    assert_eq!(
        applied(&plan, &ws.path("main.tf")),
        "terraform {\n  required_version = \">= 1.5\"\n}\n\n\
         module \"thing\" {\n  source = \"./modules/thing\"\n  region = \"eu-west-1\"\n  \
         size   = 3\n  tags = {}\n}\n"
    );
}

#[test]
fn an_addition_reparses_clean() {
    let ws = realistic();
    let thing = ws.symbol("thing", SymbolKind::Module);
    let change = signature::change(
        &ws.index,
        thing,
        Change::Add {
            at: 2,
            declaration: "variable \"tags\" {\n  type = map(string)\n}".into(),
            argument: "{}".into(),
        },
    )
    .unwrap();
    let outcomes = plan(&change.edits, Validation::ReparseStrict).unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|o| o.changed()));
}

#[test]
fn a_variable_can_be_declared_before_the_others() {
    let ws = realistic();
    let region = ws.symbol("region", SymbolKind::Variable);
    let plan = signature::change(
        &ws.index,
        region,
        Change::Add {
            at: 0,
            declaration: "variable \"name\" {\n  type = string\n}".into(),
            argument: "\"demo\"".into(),
        },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("modules/thing/variables.tf")),
        "variable \"name\" {\n  type = string\n}\n\n\
         variable \"region\" {\n  type = string\n}\n\n\
         variable \"size\" {\n  type    = number\n  default = 1\n}\n"
    );
}

#[test]
fn a_defaulted_variable_needs_no_argument_at_the_call_sites() {
    let ws = realistic();
    let thing = ws.symbol("thing", SymbolKind::Module);
    let plan = signature::change(
        &ws.index,
        thing,
        Change::Add {
            at: 9,
            declaration: "variable \"retries\" {\n  default = 3\n}".into(),
            argument: String::new(),
        },
    )
    .unwrap();
    assert_eq!(plan.call_sites, 0);
    assert_eq!(applied(&plan, &ws.path("main.tf")), ROOT_MAIN_TF);
    assert!(applied(&plan, &ws.path("modules/thing/variables.tf"))
        .ends_with("variable \"retries\" {\n  default = 3\n}\n"));
}

#[test]
fn a_required_variable_with_no_argument_to_pass_refuses() {
    let ws = realistic();
    let thing = ws.symbol("thing", SymbolKind::Module);
    let error = signature::change(
        &ws.index,
        thing,
        Change::Add {
            at: 2,
            declaration: "variable \"account\" {\n  type = string\n}".into(),
            argument: String::new(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("has no `default`"), "got: {error}");
    assert!(error.contains("1 call site(s)"), "got: {error}");
}

#[test]
fn adding_a_name_the_module_already_declares_refuses() {
    let ws = realistic();
    let thing = ws.symbol("thing", SymbolKind::Module);
    let error = refusal(
        signature::change(
            &ws.index,
            thing,
            Change::Add {
                at: 0,
                declaration: "variable \"size\" {\n  default = 2\n}".into(),
                argument: "2".into(),
            },
        )
        .unwrap_err(),
    );
    assert!(error.contains("'size' is already defined"), "got: {error}");
}

#[test]
fn adding_a_name_a_caller_already_passes_refuses() {
    // The caller sets an argument the module never declared. That configuration is
    // already broken, and adding the declaration under it would hide the fact.
    let ws = Workspace::new(&[
        (
            "main.tf",
            "module \"thing\" {\n  source = \"./modules/thing\"\n  stray  = 1\n}\n",
        ),
        (
            "modules/thing/variables.tf",
            "variable \"size\" {\n  default = 1\n}\n",
        ),
    ]);
    let thing = ws.symbol("thing", SymbolKind::Module);
    let error = refusal(
        signature::change(
            &ws.index,
            thing,
            Change::Add {
                at: 1,
                declaration: "variable \"stray\" {\n  default = 0\n}".into(),
                argument: "1".into(),
            },
        )
        .unwrap_err(),
    );
    assert!(error.contains("'stray' is already defined"), "got: {error}");
    assert!(error.contains("main.tf"), "got: {error}");
}

#[test]
fn a_module_with_no_variables_yet_needs_a_variables_file() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "module \"thing\" {\n  source = \"./modules/thing\"\n}\n",
        ),
        (
            "modules/thing/main.tf",
            "resource \"null_resource\" \"r\" {}\n",
        ),
    ]);
    let thing = ws.symbol("thing", SymbolKind::Module);
    let error = signature::change(
        &ws.index,
        thing,
        Change::Add {
            at: 0,
            declaration: "variable \"size\" {\n  default = 1\n}".into(),
            argument: "1".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("no variables.tf"), "got: {error}");
}

#[test]
fn a_module_with_an_empty_variables_file_gains_the_first_variable() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "module \"thing\" {\n  source = \"./modules/thing\"\n}\n",
        ),
        ("modules/thing/variables.tf", ""),
    ]);
    let thing = ws.symbol("thing", SymbolKind::Module);
    let plan = signature::change(
        &ws.index,
        thing,
        Change::Add {
            at: 0,
            declaration: "variable \"size\" {\n  default = 1\n}".into(),
            argument: "1".into(),
        },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("modules/thing/variables.tf")),
        "variable \"size\" {\n  default = 1\n}\n"
    );
    assert_eq!(
        applied(&plan, &ws.path("main.tf")),
        "module \"thing\" {\n  source = \"./modules/thing\"\n  size = 1\n}\n"
    );
}

#[test]
fn a_declaration_that_is_not_a_variable_block_is_rejected() {
    let ws = realistic();
    let thing = ws.symbol("thing", SymbolKind::Module);
    for (declaration, expected) in [
        ("size", "does not parse as Terraform"),
        (
            "output \"x\" {\n  value = 1\n}",
            "must be a `variable` block",
        ),
        (
            "variable \"a\" {}\nvariable \"b\" {}",
            "exactly one `variable` block",
        ),
        ("variable \"1bad\" {}", "not a valid name here"),
    ] {
        let error = signature::change(
            &ws.index,
            thing,
            Change::Add {
                at: 0,
                declaration: declaration.into(),
                argument: "1".into(),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "for {declaration:?} got: {error}");
    }
}

// --------------------------------------------------------------- refusals

#[test]
fn reordering_module_variables_is_refused_as_meaningless() {
    let ws = realistic();
    let size = ws.symbol("size", SymbolKind::Variable);
    let error =
        refusal(signature::change(&ws.index, size, Change::Move { from: 1, to: 0 }).unwrap_err());
    assert!(
        error.contains("named rather than positional"),
        "got: {error}"
    );
}

#[test]
fn a_computed_module_source_anywhere_refuses_the_change() {
    // `source = var.where` cannot be shown *not* to call this module, and a change
    // that updates the callers we can see is exactly the partial update that leaves
    // Terraform broken.
    let ws = Workspace::new(&[
        ("main.tf", ROOT_MAIN_TF),
        ("modules/thing/variables.tf", THING_VARIABLES_TF),
        ("modules/thing/main.tf", THING_MAIN_TF),
        (
            "dynamic.tf",
            "module \"mystery\" {\n  source = var.where\n}\n",
        ),
    ]);
    let size = ws.symbol("size", SymbolKind::Variable);
    let error = refusal(signature::change(&ws.index, size, Change::Remove(1)).unwrap_err());
    assert!(
        error.contains("do not name a literal source"),
        "got: {error}"
    );
    assert!(
        error.contains("computed source `var.where`"),
        "got: {error}"
    );
    assert!(error.contains("dynamic.tf:1"), "got: {error}");
}

#[test]
fn a_module_block_with_no_source_refuses_the_change() {
    let ws = Workspace::new(&[
        ("main.tf", ROOT_MAIN_TF),
        ("modules/thing/variables.tf", THING_VARIABLES_TF),
        ("modules/thing/main.tf", THING_MAIN_TF),
        ("stray.tf", "module \"nowhere\" {\n  count = 1\n}\n"),
    ]);
    let size = ws.symbol("size", SymbolKind::Variable);
    let error = refusal(signature::change(&ws.index, size, Change::Remove(1)).unwrap_err());
    assert!(error.contains("has no `source` argument"), "got: {error}");
}

#[test]
fn a_caller_that_is_not_a_top_level_block_refuses_the_change() {
    // The index records the nested block's `source` as an import, so the call
    // surface is provably wider than what this rewrite can reach.
    let ws = Workspace::new(&[
        (
            "main.tf",
            "resource \"null_resource\" \"wrapper\" {\n  \
             module \"thing\" {\n    source = \"./modules/thing\"\n  }\n}\n",
        ),
        (
            "modules/thing/variables.tf",
            "variable \"size\" {\n  default = 1\n}\n",
        ),
    ]);
    let size = ws.symbol("size", SymbolKind::Variable);
    let error = refusal(signature::change(&ws.index, size, Change::Remove(0)).unwrap_err());
    assert!(error.contains("not a top-level block"), "got: {error}");
}

#[test]
fn a_registry_module_has_no_signature_here() {
    let ws = Workspace::new(&[(
        "main.tf",
        "module \"vpc\" {\n  source  = \"terraform-aws-modules/vpc/aws\"\n  version = \"5.0.0\"\n}\n",
    )]);
    let vpc = ws.symbol("vpc", SymbolKind::Module);
    let error = signature::change(&ws.index, vpc, Change::Remove(0))
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a local directory"), "got: {error}");
}

#[test]
fn a_module_source_pointing_nowhere_is_unresolvable() {
    let ws = Workspace::new(&[(
        "main.tf",
        "module \"gone\" {\n  source = \"./missing\"\n}\n",
    )]);
    let gone = ws.symbol("gone", SymbolKind::Module);
    let error = signature::change(&ws.index, gone, Change::Remove(0))
        .unwrap_err()
        .to_string();
    assert!(error.contains("holds no Terraform files"), "got: {error}");
}

#[test]
fn only_variable_and_module_blocks_name_a_signature() {
    let ws = realistic();
    // A `resource` block is addressable but has no call surface to change.
    let resource = ws.symbol("r", SymbolKind::Block);
    let error = signature::change(&ws.index, resource, Change::Remove(0))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("only a `variable` block or a `module` block"),
        "got: {error}"
    );
}

#[test]
fn a_locals_entry_is_not_a_module_variable() {
    // `locals { x = 1 }` also extracts as a Variable, but it is internal to the
    // module and no caller can pass it.
    let ws = Workspace::new(&[("main.tf", "locals {\n  helper = 1\n}\n")]);
    let helper = ws.symbol("helper", SymbolKind::Variable);
    let error = signature::change(&ws.index, helper, Change::Remove(0))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("only a `variable` block or a `module` block"),
        "got: {error}"
    );
}

// ------------------------------------------------------------------ SCSS

#[test]
fn scss_mixins_produce_no_symbols_to_change() {
    // The evidence for the refusal below: `@mixin`/`@include` is not in the grammar
    // SCSS is parsed with, so there is nothing to take a signature from.
    let src = "@mixin theme($c, $d) {\n  color: $c;\n}\n.btn {\n  @include theme(red, blue);\n}\n";
    let parsed = Parsers::new().parse(Language::Scss, src).unwrap();
    assert!(
        parsed.has_errors(),
        "@mixin/@include should surface as parse errors, not silent success"
    );

    let ws = Workspace::new(&[("theme.scss", src)]);
    assert!(
        ws.index.find_symbols("theme", None).is_empty(),
        "a mixin yields no symbol: {:?}",
        ws.index.find_symbols("theme", None)
    );
    // Not even the parameters exist as names.
    assert!(ws.index.find_symbols("$c", None).is_empty());
    assert!(ws.index.find_symbols("c", None).is_empty());
}

#[test]
fn changing_an_scss_signature_names_the_grammar_limitation() {
    // The only SCSS symbols that exist are the CSS ones, so that is the handle a
    // user would reach for. It must refuse, and say why rather than "unsupported".
    let ws = Workspace::new(&[(
        "theme.scss",
        "@mixin theme($c) {\n  color: $c;\n}\n.btn {\n  @include theme(red);\n}\n",
    )]);
    let btn = ws.symbol("btn", SymbolKind::Selector);
    let error = refusal(signature::change(&ws.index, btn, Change::Remove(0)).unwrap_err());
    assert!(error.contains("changing mixin parameters"), "got: {error}");
    assert!(error.contains("tree-sitter-css grammar"), "got: {error}");
    assert!(error.contains("`@mixin`"), "got: {error}");
    assert!(error.contains("`@include`"), "got: {error}");
    assert!(error.contains("no parameter list"), "got: {error}");
}

// ----------------------------------------------------------- no regression

#[test]
fn function_signatures_still_work_the_same_way() {
    let ws = Workspace::new(&[(
        "a.rs",
        "fn f(a: i32, b: i32, c: i32) {}\nfn caller() { f(1, 2, 3); }\n",
    )]);
    let f = ws.symbol("f", SymbolKind::Function);
    let plan = signature::change(&ws.index, f, Change::Remove(1)).unwrap();
    assert_eq!(plan.subject_kind, Subject::Callable);
    assert_eq!(plan.subject, "f");
    assert_eq!(
        applied(&plan, &ws.path("a.rs")),
        "fn f(a: i32, c: i32) {}\nfn caller() { f(1, 3); }\n"
    );
}

#[test]
fn the_summary_says_module_variable_for_terraform() {
    let ws = realistic();
    let size = ws.symbol("size", SymbolKind::Variable);
    let plan = signature::change(&ws.index, size, Change::Remove(1)).unwrap();
    let summary = signature::describe(&ws.index, &plan);
    assert!(
        summary.contains("removed module variable 1"),
        "got: {summary}"
    );
    assert!(summary.contains("1 call site(s)"), "got: {summary}");
    assert!(summary.contains("modules/thing"), "got: {summary}");
}

