//! Two declarations that share a name in different namespaces.
//!
//! Terraform writes the namespace in front of the use: `var.thing` and `local.thing` are
//! two addresses, and a module may declare both. Markup writes it in the attribute:
//! `class="thing"` names a CSS class and `href="#thing"` names an element id.
//!
//! The index recorded both pairs as one name each, so `fr refs` on either declaration
//! answered for both, and a rename of one rewrote the uses of the other.

use fun_refactor::index::Index;
use fun_refactor::model::{SymbolId, SymbolKind};
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

fn only_of_kind(index: &Index, name: &str, kind: SymbolKind, file: &str) -> SymbolId {
    let found: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.kind == kind && s.file.ends_with(file))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected one {kind:?} named {name} in {file}: {found:?}"
    );
    found[0].id
}

const TERRAFORM: &str = "variable \"thing\" {\n  type = string\n}\n\n\
                         locals {\n  thing = \"computed\"\n}\n\n\
                         output \"from_var\" {\n  value = var.thing\n}\n\n\
                         output \"from_local\" {\n  value = local.thing\n}\n";

#[test]
fn a_terraform_variable_and_a_local_are_two_declarations() {
    let (_tmp, index) = workspace(&[("main.tf", TERRAFORM)]);
    let mut variables: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.name == "thing" && s.kind == SymbolKind::Variable)
        .collect();
    variables.sort_by_key(|s| s.full_span.start);
    assert_eq!(variables.len(), 2, "the module declares both");

    // The `variable` block is addressed as `var.thing`, the `locals` entry as
    // `local.thing`. Each has exactly one use here.
    for (symbol, expected) in [(variables[0], "var.thing"), (variables[1], "local.thing")] {
        let uses = index.references_to(symbol.id);
        assert_eq!(uses.len(), 1, "{expected}: {uses:?}");
        assert!(
            uses[0].confidence.is_safe_to_rewrite(),
            "{expected} resolved only as {:?}",
            uses[0].confidence
        );
    }
}

const CSS: &str = ".thing { color: red; }\n#thing { color: blue; }\n";
const HTML: &str =
    "<div class=\"thing\">a</div>\n<div id=\"thing\">b</div>\n<a href=\"#thing\">c</a>\n";

#[test]
fn a_css_class_and_an_element_id_are_two_declarations() {
    let (_tmp, index) = workspace(&[("s.css", CSS), ("p.html", HTML)]);

    let class = only_of_kind(&index, "thing", SymbolKind::Selector, "s.css");
    let uses = index.references_to(class);
    assert_eq!(
        uses.len(),
        1,
        "the class is used by the class attribute: {uses:?}"
    );
    assert!(uses[0].file.ends_with("p.html"));

    let id = only_of_kind(&index, "thing", SymbolKind::ElementId, "s.css");
    let uses = index.references_to(id);
    assert_eq!(
        uses.len(),
        1,
        "the id is used by the fragment link: {uses:?}"
    );
}

#[test]
fn renaming_an_element_id_leaves_the_class_of_that_name_alone() {
    // Before: renaming `#thing` rewrote `class="thing"` as well, so the element lost the
    // class that styled it and gained one that nothing declares.
    let (tmp, index) = workspace(&[("s.css", CSS), ("p.html", HTML)]);
    let id = only_of_kind(&index, "thing", SymbolKind::ElementId, "s.css");
    let plan = rename::plan(&index, id, "renamed").expect("a plan");

    let html = tmp.path().join("p.html");
    let after = fun_refactor::edit::apply_to_string(HTML, plan.edits.edits_for(&html).unwrap())
        .expect("the edits apply");
    assert!(
        after.contains("class=\"thing\""),
        "the class was rewritten:\n{after}"
    );
    assert!(after.contains("id=\"renamed\""), "{after}");
    assert!(after.contains("href=\"#renamed\""), "{after}");
}

const CALLER_TF: &str = "variable \"region\" {\n  type = string\n}\n\n\
                         module \"net\" {\n  source = \"./modules/net\"\n  \
                         region = var.region\n}\n";

const CALLED_TF: &str = "variable \"region\" {\n  type = string\n}\n\n\
                         output \"where\" {\n  value = var.region\n}\n";

/// An argument of a `module` block names an input variable of the called configuration.
/// Renaming that variable left `region = var.region` behind and reported success, and
/// `terraform validate` then rejected the result.
#[test]
fn a_module_argument_renames_with_the_variable_it_names() {
    let (tmp, index) = workspace(&[("main.tf", CALLER_TF), ("modules/net/main.tf", CALLED_TF)]);
    let called = only_of_kind(
        &index,
        "region",
        SymbolKind::Variable,
        "modules/net/main.tf",
    );
    let plan = rename::plan(&index, called, "vpc_region").expect("a plan");

    let caller = tmp.path().join("main.tf");
    let after =
        fun_refactor::edit::apply_to_string(CALLER_TF, plan.edits.edits_for(&caller).unwrap())
            .expect("the edits apply");
    assert!(after.contains("vpc_region = var.region"), "{after}");
    assert!(
        after.contains("variable \"region\""),
        "the caller's own variable is another declaration:\n{after}"
    );
}

/// The caller's own `variable "region"` is a different declaration, and its rename must
/// leave the module's call surface alone.
#[test]
fn renaming_the_callers_variable_leaves_the_module_argument_alone() {
    let (tmp, index) = workspace(&[("main.tf", CALLER_TF), ("modules/net/main.tf", CALLED_TF)]);
    let caller_variable = index
        .symbols
        .iter()
        .find(|s| {
            s.name == "region"
                && s.kind == SymbolKind::Variable
                && s.file == tmp.path().join("main.tf")
        })
        .expect("the caller's own variable")
        .id;
    let plan = rename::plan(&index, caller_variable, "where").expect("a plan");

    let caller = tmp.path().join("main.tf");
    let after =
        fun_refactor::edit::apply_to_string(CALLER_TF, plan.edits.edits_for(&caller).unwrap())
            .expect("the edits apply");
    assert!(after.contains("region = var.where"), "{after}");
    assert!(after.contains("variable \"where\""), "{after}");
}

/// A source outside the workspace names a configuration nothing here can read, so the
/// argument stays unresolved and is reported rather than rewritten.
#[test]
fn an_argument_of_a_module_from_the_registry_is_reported() {
    let caller = "module \"net\" {\n  source = \"./modules/net\"\n  region = \"eu-west-1\"\n}\n\n\
                  module \"vpc\" {\n  source = \"terraform-aws-modules/vpc/aws\"\n  \
                  region = \"eu-west-1\"\n}\n";
    let (_tmp, index) = workspace(&[
        ("main.tf", caller),
        (
            "modules/net/main.tf",
            "variable \"region\" {\n  type = string\n}\n",
        ),
    ]);
    let called = only_of_kind(
        &index,
        "region",
        SymbolKind::Variable,
        "modules/net/main.tf",
    );
    let plan = rename::plan(&index, called, "vpc_region").expect("a plan");

    let reported: Vec<&str> = plan.warnings.iter().map(|w| w.detail.as_str()).collect();
    assert!(
        reported.iter().any(|d| d.contains("module \"vpc\"")),
        "the unhandled argument should be named: {reported:?}"
    );
}

const CALLER_READING_AN_OUTPUT: &str = "module \"net\" {\n  source = \"./modules/net\"\n}\n\n\
                                        output \"id\" {\n  value = module.net.subnet_id\n}\n";

const MODULE_WITH_AN_OUTPUT: &str = "output \"subnet_id\" {\n  value = \"s-1\"\n}\n\n\
                                     provider \"subnet_id\" {\n  region = \"eu-west-1\"\n}\n";

/// `module.net.subnet_id` is a reference to the module's `output "subnet_id"`.
///
/// `fr flow` and `fr signature` followed this edge from their own code. The index held
/// nothing, so `fr usages`, `fr refs` and `fr impact` answered zero for a name the
/// workspace reads. Every command reads the index, so the edge belongs there.
#[test]
fn a_module_output_read_from_outside_is_a_reference_in_the_index() {
    let (tmp, index) = workspace(&[
        ("main.tf", CALLER_READING_AN_OUTPUT),
        ("modules/net/main.tf", MODULE_WITH_AN_OUTPUT),
    ]);
    let output = index
        .symbols
        .iter()
        .find(|s| s.name == "subnet_id" && s.qualifier.as_deref() == Some("output"))
        .expect("the module's output")
        .id;

    let uses = index.references_to(output);
    assert_eq!(uses.len(), 1, "one read of the output: {uses:?}");
    assert_eq!(uses[0].file, tmp.path().join("main.tf"));
    assert_eq!(
        uses[0].confidence,
        fun_refactor::model::Confidence::Exact,
        "the module's source names the directory, so nothing is inferred"
    );
}

/// A `provider "subnet_id"` in the same directory is a different declaration, and the
/// block-type keyword written in front of each is what says so.
#[test]
fn a_module_output_is_told_from_a_block_of_another_type_with_the_same_name() {
    let (_tmp, index) = workspace(&[
        ("main.tf", CALLER_READING_AN_OUTPUT),
        ("modules/net/main.tf", MODULE_WITH_AN_OUTPUT),
    ]);
    let provider = index
        .symbols
        .iter()
        .find(|s| s.name == "subnet_id" && s.qualifier.as_deref() == Some("provider"))
        .expect("the provider block")
        .id;
    assert!(
        index.references_to(provider).is_empty(),
        "no traversal reaches a provider block"
    );
}

/// Deleting an output something still reads leaves `terraform validate` failing. The
/// refusal already existed for Helm values and CSS classes; it now covers this too,
/// because it reads the same reference edge.
#[test]
fn deleting_a_module_output_that_is_still_read_is_refused() {
    let (_tmp, index) = workspace(&[
        ("main.tf", CALLER_READING_AN_OUTPUT),
        ("modules/net/main.tf", MODULE_WITH_AN_OUTPUT),
    ]);
    let output = index
        .symbols
        .iter()
        .find(|s| s.name == "subnet_id" && s.qualifier.as_deref() == Some("output"))
        .expect("the module's output")
        .id;
    let refusal = fun_refactor::refactor::delete::plan(&index, output)
        .expect_err("a read output cannot be deleted");
    let said = refusal.to_string();
    assert!(
        said.contains("still resolve to it"),
        "the refusal names the uses: {said}"
    );
}

/// Renaming the output rewrites the read in the calling module.
#[test]
fn renaming_a_module_output_rewrites_the_caller() {
    let (tmp, index) = workspace(&[
        ("main.tf", CALLER_READING_AN_OUTPUT),
        ("modules/net/main.tf", MODULE_WITH_AN_OUTPUT),
    ]);
    let output = index
        .symbols
        .iter()
        .find(|s| s.name == "subnet_id" && s.qualifier.as_deref() == Some("output"))
        .expect("the module's output")
        .id;
    let plan = rename::plan(&index, output, "net_id").expect("a plan");

    let caller = tmp.path().join("main.tf");
    let after = fun_refactor::edit::apply_to_string(
        CALLER_READING_AN_OUTPUT,
        plan.edits.edits_for(&caller).unwrap(),
    )
    .expect("the edits apply");
    assert!(after.contains("module.net.net_id"), "{after}");
}

#[test]
fn renaming_a_css_class_leaves_the_id_of_that_name_alone() {
    let (tmp, index) = workspace(&[("s.css", CSS), ("p.html", HTML)]);
    let class = only_of_kind(&index, "thing", SymbolKind::Selector, "s.css");
    let plan = rename::plan(&index, class, "renamed").expect("a plan");

    let html = tmp.path().join("p.html");
    let after = fun_refactor::edit::apply_to_string(HTML, plan.edits.edits_for(&html).unwrap())
        .expect("the edits apply");
    assert!(after.contains("class=\"renamed\""), "{after}");
    assert!(
        after.contains("id=\"thing\""),
        "the id was rewritten:\n{after}"
    );
    assert!(
        after.contains("href=\"#thing\""),
        "the link was rewritten:\n{after}"
    );
}
