//! Terraform/HCL fact extraction, exercised through the public API.
//!
//! Terraform has no lexical binding: a declaration is its string labels and a use
//! site is a scope-traversal expression. These tests pin down which byte range of
//! each construct a rename would rewrite, since that is the whole point of the
//! extraction and the part the grammar makes easy to get subtly wrong.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

fn hcl(src: &str) -> FileFacts {
    facts(Language::Hcl, src)
}

fn sym<'a>(f: &'a FileFacts, name: &str) -> &'a Symbol {
    f.symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}: {:?}", names(f)))
}

fn names(f: &FileFacts) -> Vec<(&str, SymbolKind)> {
    f.symbols
        .iter()
        .map(|s| (s.name.as_str(), s.kind))
        .collect()
}

fn refs<'a>(f: &'a FileFacts, name: &str) -> Vec<&'a Reference> {
    f.references.iter().filter(|r| r.name == name).collect()
}

const MAIN_TF: &str = r#"terraform {
  required_version = ">= 1.5"
}

provider "aws" {
  region = var.region
}

variable "bucket_name" {
  type    = string
  default = "demo"
}

variable "region" {
  type = string
}

locals {
  prefix = "app"
  tags   = { Name = local.prefix }
}

resource "aws_s3_bucket" "main" {
  bucket = "${local.prefix}-${var.bucket_name}"
  tags   = local.tags
}

data "aws_caller_identity" "current" {}

module "network" {
  source = "./modules/network"
  cidr   = var.cidr
}

output "bucket_arn" {
  value = aws_s3_bucket.main.arn
}

output "acct" {
  value = data.aws_caller_identity.current.account_id
}

output "net" {
  value = module.network.vpc_id
}

resource "aws_instance" "web" {
  for_each = var.instances
  name     = each.value
  count    = length(var.list)
}
"#;

#[test]
fn realistic_configuration_parses_cleanly() {
    let parsed = Parsers::new().parse(Language::Hcl, MAIN_TF).unwrap();
    assert!(
        !parsed.has_errors(),
        "main.tf should parse without errors: {:?}",
        parsed.error_spans()
    );
}

// ------------------------------------------------------------- definitions

#[test]
fn variable_blocks_are_variables_named_without_quotes() {
    let f = hcl(MAIN_TF);
    let v = sym(&f, "bucket_name");
    assert_eq!(v.kind, SymbolKind::Variable);
    // The label is `"bucket_name"` in the source; the name must be the content
    // only, or a rename would write quotes inside quotes.
    assert_eq!(v.name_span.text(MAIN_TF), "bucket_name");
    assert_eq!(MAIN_TF.as_bytes()[v.name_span.start - 1], b'"');
    assert_eq!(MAIN_TF.as_bytes()[v.name_span.end], b'"');
    // The full span is the whole block, body included.
    assert!(v
        .full_span
        .text(MAIN_TF)
        .starts_with("variable \"bucket_name\""));
    assert!(v.full_span.text(MAIN_TF).ends_with('}'));
    assert!(v.full_span.contains(v.name_span));
}

#[test]
fn locals_attributes_are_variables() {
    let f = hcl(MAIN_TF);
    for name in ["prefix", "tags"] {
        let s = sym(&f, name);
        assert_eq!(s.kind, SymbolKind::Variable, "{name}");
        assert_eq!(s.name_span.text(MAIN_TF), name);
    }
    // The whole attribute is the definition, so deleting an unused local removes
    // its value too.
    assert_eq!(
        sym(&f, "prefix").full_span.text(MAIN_TF),
        r#"prefix = "app""#
    );
}

#[test]
fn attributes_outside_locals_are_not_definitions() {
    // `bucket` and `region` are provider-defined arguments, not addresses.
    let f = hcl(MAIN_TF);
    assert!(
        !f.symbols.iter().any(|s| s.name == "bucket"),
        "resource arguments must not become symbols: {:?}",
        names(&f)
    );
    assert!(
        f.symbols.iter().all(|s| s.name != "required_version"),
        "block arguments must not become symbols: {:?}",
        names(&f)
    );
}

#[test]
fn resource_and_data_carry_the_terraform_address() {
    let f = hcl(MAIN_TF);
    let bucket = sym(&f, "main");
    assert_eq!(bucket.kind, SymbolKind::Block);
    // The renameable byte range is the name label alone...
    assert_eq!(bucket.name_span.text(MAIN_TF), "main");
    // ...while the type label qualifies it, reconstructing `aws_s3_bucket.main`
    // with the engine's `::` separator.
    assert_eq!(bucket.qualifier.as_deref(), Some("aws_s3_bucket"));
    assert_eq!(bucket.qualified_name(), "aws_s3_bucket::main");

    let ident = sym(&f, "current");
    assert_eq!(ident.qualified_name(), "aws_caller_identity::current");
}

#[test]
fn type_label_of_a_resource_is_not_a_second_symbol() {
    // Renaming a resource must have exactly one definition site. The type label
    // is a container, not a definition, exactly as Rust's `impl S` is.
    let f = hcl(MAIN_TF);
    assert!(
        !f.symbols.iter().any(|s| s.name == "aws_s3_bucket"),
        "the type label must not define a symbol: {:?}",
        names(&f)
    );
}

#[test]
fn module_blocks_are_modules_and_outputs_are_blocks() {
    let f = hcl(MAIN_TF);
    assert_eq!(sym(&f, "network").kind, SymbolKind::Module);
    // `output` is a boundary declaration consumed from outside the module, so it
    // is a Block rather than a Variable.
    for out in ["bucket_arn", "acct", "net"] {
        assert_eq!(sym(&f, out).kind, SymbolKind::Block, "{out}");
    }
}

#[test]
fn unlabelled_and_single_label_blocks_are_named_by_what_they_have() {
    let f = hcl(MAIN_TF);
    // `terraform {}` has no label; its keyword is its only name.
    let tf = sym(&f, "terraform");
    assert_eq!(tf.kind, SymbolKind::Block);
    assert_eq!(tf.name_span.text(MAIN_TF), "terraform");
    // `provider "aws"` is named by its single label.
    let aws = sym(&f, "aws");
    assert_eq!(aws.kind, SymbolKind::Block);
    assert_eq!(aws.name_span.text(MAIN_TF), "aws");
}

#[test]
fn each_block_yields_exactly_one_symbol() {
    // Label arity is matched structurally, so a two-label block must not also
    // match the one-label pattern (which would double every resource).
    let f = hcl(MAIN_TF);
    for name in ["main", "web", "current", "network", "bucket_arn", "aws"] {
        let hits = f.symbols.iter().filter(|s| s.name == name).count();
        assert_eq!(hits, 1, "{name} produced {hits} symbols: {:?}", names(&f));
    }
}

#[test]
fn nested_blocks_nest_by_containment() {
    let src = r#"resource "aws_instance" "web" {
  lifecycle {
    create_before_destroy = true
  }
}
"#;
    let f = hcl(src);
    let web = sym(&f, "web");
    let lifecycle = sym(&f, "lifecycle");
    assert_eq!(lifecycle.container, Some(web.id));
    // The resource's type label qualifies what it encloses too.
    assert_eq!(lifecycle.qualifier.as_deref(), Some("aws_instance"));
}

// -------------------------------------------------------------- references

#[test]
fn var_and_local_references_name_the_declaration() {
    // This is the reference form rename depends on most: `var.region` must yield
    // a reference literally named `region`, spanning only that segment.
    let f = hcl(MAIN_TF);
    let region = refs(&f, "region");
    assert_eq!(region.len(), 1, "got {region:?}");
    assert_eq!(region[0].kind, ReferenceKind::Identifier);
    assert_eq!(region[0].span.text(MAIN_TF), "region");
    // The span starts after the dot, so a rename never touches `var.`.
    assert_eq!(MAIN_TF.as_bytes()[region[0].span.start - 1], b'.');

    let tags = refs(&f, "tags");
    assert_eq!(tags.len(), 1, "local.tags: got {tags:?}");
    assert_eq!(tags[0].kind, ReferenceKind::Identifier);
}

#[test]
fn declaration_labels_are_not_also_references() {
    // `prefix` is written three times: once as a local declaration and twice as
    // `local.prefix`. Only the two uses are references.
    let f = hcl(MAIN_TF);
    let prefix = refs(&f, "prefix");
    assert_eq!(prefix.len(), 2, "got {prefix:?}");
    let def_start = sym(&f, "prefix").name_span.start;
    assert!(prefix.iter().all(|r| r.span.start != def_start));
}

#[test]
fn interpolated_references_are_found_inside_strings() {
    // `"${local.prefix}-${var.bucket_name}"` is where most Terraform references
    // actually live.
    let f = hcl(MAIN_TF);
    let interp = MAIN_TF.find("\"${local.prefix}").unwrap();
    let end = MAIN_TF[interp..].find('\n').unwrap() + interp;
    let inside: Vec<_> = f
        .references
        .iter()
        .filter(|r| r.span.start > interp && r.span.end < end)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(inside, vec!["prefix", "bucket_name"], "got {inside:?}");
}

#[test]
fn managed_resource_address_splits_into_type_and_name() {
    // `aws_s3_bucket.main.arn`: the name is renameable, the type is provider-fixed
    // but still rewritten by a type change, and `arn` is a read of the resource.
    let f = hcl(MAIN_TF);
    // `seg` is written with its leading dot to disambiguate; the reference span
    // deliberately starts after that dot, so step over it.
    let at = |needle: &str, seg: &str| {
        let base = MAIN_TF.find(needle).unwrap();
        let off = base + MAIN_TF[base..].find(seg).unwrap() + usize::from(seg.starts_with('.'));
        f.reference_at(off)
            .unwrap_or_else(|| panic!("no reference at {seg} in {needle}"))
    };
    let ty = at("aws_s3_bucket.main.arn", "aws_s3_bucket");
    assert_eq!(
        (ty.name.as_str(), ty.kind),
        ("aws_s3_bucket", ReferenceKind::Type)
    );
    let name = at("aws_s3_bucket.main.arn", ".main");
    assert_eq!(
        (name.name.as_str(), name.kind),
        ("main", ReferenceKind::Identifier)
    );
    let attr = at("aws_s3_bucket.main.arn", ".arn");
    assert_eq!(
        (attr.name.as_str(), attr.kind),
        ("arn", ReferenceKind::Field)
    );
}

#[test]
fn data_address_resolves_the_name_one_segment_further_along() {
    // `data.TYPE.NAME.attr` is one segment longer than a managed resource
    // address; the renameable segment is the third, not the second.
    let f = hcl(MAIN_TF);
    let expr = MAIN_TF
        .find("data.aws_caller_identity.current.account_id")
        .unwrap();
    let at = |seg: &str| {
        f.reference_at(expr + MAIN_TF[expr..].find(seg).unwrap() + 1)
            .unwrap_or_else(|| panic!("no reference at {seg}"))
    };
    let ty = at(".aws_caller_identity");
    assert_eq!(ty.name, "aws_caller_identity");
    assert_eq!(ty.kind, ReferenceKind::Type);
    let name = at(".current");
    assert_eq!(name.name, "current");
    assert_eq!(name.kind, ReferenceKind::Identifier);
    let attr = at(".account_id");
    assert_eq!(attr.name, "account_id");
    assert_eq!(attr.kind, ReferenceKind::Field);
    // The `data` namespace keyword itself is never a reference.
    assert!(f.reference_at(expr).is_none());
}

#[test]
fn module_output_reference_names_the_module_then_the_output() {
    let f = hcl(MAIN_TF);
    let expr = MAIN_TF.find("module.network.vpc_id").unwrap();
    let m = f.reference_at(expr + 7).unwrap();
    assert_eq!(m.name, "network");
    assert_eq!(m.kind, ReferenceKind::Identifier);
    // Resolving `vpc_id` to an `output "vpc_id"` needs the other module's file,
    // so here it is only a field read.
    let out = f.reference_at(expr + 15).unwrap();
    assert_eq!(out.name, "vpc_id");
    assert_eq!(out.kind, ReferenceKind::Field);
}

#[test]
fn evaluation_context_values_are_fields_not_renameable_names() {
    // `each.value` has no declaration site anywhere, so it must not be reported
    // as an identifier a rename could chase.
    let f = hcl(MAIN_TF);
    let value = refs(&f, "value");
    assert_eq!(value.len(), 1, "got {value:?}");
    assert_eq!(value[0].kind, ReferenceKind::Field);
    assert!(
        refs(&f, "each").is_empty(),
        "`each` is a namespace, not a name"
    );
}

#[test]
fn more_evaluation_namespaces() {
    let src = r#"resource "x" "y" {
  a = count.index
  b = self.id
  c = path.module
  d = terraform.workspace
}
"#;
    let f = hcl(src);
    for (seg, name) in [
        ("count.index", "index"),
        ("self.id", "id"),
        ("path.module", "module"),
        ("terraform.workspace", "workspace"),
    ] {
        let r = refs(&f, name);
        assert_eq!(r.len(), 1, "{seg}: got {r:?}");
        assert_eq!(r[0].kind, ReferenceKind::Field, "{seg}");
    }
    // None of the namespace roots leak in as bare references.
    for root in ["count", "self", "path"] {
        assert!(
            refs(&f, root).is_empty(),
            "{root} leaked: {:?}",
            f.references
        );
    }
}

#[test]
fn function_calls_are_calls() {
    let f = hcl(MAIN_TF);
    let length = refs(&f, "length");
    assert_eq!(length.len(), 1, "got {length:?}");
    assert_eq!(length[0].kind, ReferenceKind::Call);
    // Its argument is still resolved as a variable reference.
    assert_eq!(refs(&f, "list").len(), 1);
}

#[test]
fn every_reference_starts_unresolved() {
    // Resolution needs the whole module, which the extractor cannot see.
    let f = hcl(MAIN_TF);
    assert!(!f.references.is_empty());
    assert!(f
        .references
        .iter()
        .all(|r| r.target.is_none() && r.confidence == Confidence::NameOnly));
}

// ----------------------------------------------------------------- imports

#[test]
fn module_source_is_the_import() {
    let f = hcl(MAIN_TF);
    assert_eq!(f.imports.len(), 1, "got {:?}", f.imports);
    let i = &f.imports[0];
    assert_eq!(i.path, "./modules/network");
    // The module label is the local binding the imported surface is reached
    // through: `module.network.<output>`.
    assert_eq!(i.alias.as_deref(), Some("network"));
    assert!(!i.is_glob);
    assert!(i.span.text(MAIN_TF).starts_with("module \"network\""));
}

#[test]
fn registry_and_git_module_sources_are_imports_too() {
    let src = r#"module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "5.0.0"
}

module "app" {
  source = "git::https://example.com/app.git?ref=v1"
}
"#;
    let f = hcl(src);
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "terraform-aws-modules/vpc/aws",
            "git::https://example.com/app.git?ref=v1"
        ]
    );
}

#[test]
fn a_module_without_a_literal_source_yields_no_import() {
    // A computed source is not a path we can follow; reporting a bogus one would
    // be worse than reporting nothing.
    let f = hcl("module \"m\" {\n  source = var.where\n}\n");
    assert!(f.imports.is_empty(), "got {:?}", f.imports);
    assert_eq!(sym(&f, "m").kind, SymbolKind::Module);
}

// ------------------------------------------------------------------ scopes

#[test]
fn block_bodies_are_nested_scopes() {
    let f = hcl(MAIN_TF);
    let inside_bucket = MAIN_TF.find("bucket = ").unwrap();
    let inside_provider = MAIN_TF.find("region = var.region").unwrap();
    let bucket_scope = f.scope_at(inside_bucket).unwrap();
    let provider_scope = f.scope_at(inside_provider).unwrap();
    assert_ne!(bucket_scope, provider_scope);
    // Both nest inside the file scope.
    let root = f.scope_at(0).unwrap();
    assert!(f.scope_chain(bucket_scope).contains(&root));
    assert!(f.scope_chain(provider_scope).contains(&root));
}

#[test]
fn object_expressions_open_their_own_scope() {
    let f = hcl(MAIN_TF);
    let obj = MAIN_TF.find("{ Name = local.prefix }").unwrap();
    let attr = MAIN_TF.find("tags   = {").unwrap();
    let inner = f.scope_at(obj + 2).unwrap();
    let outer = f.scope_at(attr).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
}

// ------------------------------------------------------------- known gaps

#[test]
fn empty_labels_define_nothing() {
    // An empty `""` label has no `template_literal` child at all, so there is no
    // byte range to rename and no symbol is produced. Terraform rejects such a
    // configuration anyway; this test records the behaviour rather than a claim.
    let f = hcl("resource \"aws_s3_bucket\" \"\" {}\n");
    assert!(f.symbols.is_empty(), "got {:?}", names(&f));
}

#[test]
fn a_splat_keeps_its_trailing_segments() {
    // `aws_instance.web[*].id` hangs the trailing steps off a `splat` node instead of
    // continuing the flat `get_attr` run, so the sibling-anchored patterns stop at the
    // address. Matching inside the splat recovers the attribute read; it is a field,
    // exactly as it would be without the `[*]`.
    let src = "output \"ids\" {\n  value = aws_instance.web[*].id\n}\n";
    let f = hcl(src);
    let web = refs(&f, "web");
    assert_eq!(web.len(), 1, "the address must still resolve: {web:?}");
    assert_eq!(web[0].kind, ReferenceKind::Identifier);
    let id = refs(&f, "id");
    assert_eq!(id.len(), 1, "got {:?}", f.references);
    assert_eq!(id[0].kind, ReferenceKind::Field);
    assert_eq!(id[0].span.text(src), "id");
}

#[test]
fn a_legacy_attr_splat_keeps_its_trailing_segments_too() {
    // `.*.` is the older spelling and lands under `attr_splat` rather than
    // `full_splat`, which is a different node kind and so a different pattern.
    let src = "output \"ids\" {\n  value = aws_instance.web.*.id\n}\n";
    let f = hcl(src);
    let id = refs(&f, "id");
    assert_eq!(id.len(), 1, "got {:?}", f.references);
    assert_eq!(id[0].kind, ReferenceKind::Field);
}

#[test]
fn a_splat_keeps_every_trailing_segment_not_just_the_first() {
    let src = "output \"ids\" {\n  value = aws_instance.web[*].id.name\n}\n";
    let f = hcl(src);
    for segment in ["id", "name"] {
        let r = refs(&f, segment);
        assert_eq!(r.len(), 1, "{segment}: got {:?}", f.references);
        assert_eq!(r[0].kind, ReferenceKind::Field, "{segment}");
    }
}

#[test]
fn an_index_keeps_the_segments_that_follow_it() {
    // `x.y[0].z` does leave `.z` as a flat sibling, but the `index` node between it
    // and the root breaks the anchored run, so the address resolved and the attribute
    // read did not. `y` must stay the renameable identifier, not become a field.
    let src = "output \"a\" {\n  value = x.y[0].z\n}\n";
    let f = hcl(src);
    let y = refs(&f, "y");
    assert_eq!(y.len(), 1, "got {:?}", f.references);
    assert_eq!(y[0].kind, ReferenceKind::Identifier, "the address is renameable");
    let z = refs(&f, "z");
    assert_eq!(z.len(), 1, "got {:?}", f.references);
    assert_eq!(z[0].kind, ReferenceKind::Field);
}

#[test]
fn an_index_keeps_two_segments_but_not_a_third() {
    // Each step past an index needs its own pattern, and two is where this stops. The
    // third segment is a known partial: the address and the first reads survive, so
    // nothing is *wrong*, only incomplete.
    let src = "output \"a\" {\n  value = x.y[0].z.w.q\n}\n";
    let f = hcl(src);
    assert_eq!(refs(&f, "z").len(), 1, "got {:?}", f.references);
    assert_eq!(refs(&f, "w").len(), 1, "got {:?}", f.references);
    assert!(
        refs(&f, "q").is_empty(),
        "the third step past an index is not captured: {:?}",
        f.references
    );
}

#[test]
fn an_index_expression_is_still_read_as_a_traversal() {
    // `count.index` inside the brackets is its own expression and keeps its own
    // reference; the splat patterns must not swallow it.
    let src = "output \"a\" {\n  value = aws_instance.web[count.index].id\n}\n";
    let f = hcl(src);
    assert_eq!(refs(&f, "web")[0].kind, ReferenceKind::Identifier);
    assert_eq!(refs(&f, "index")[0].kind, ReferenceKind::Field);
    assert_eq!(refs(&f, "id")[0].kind, ReferenceKind::Field);
}

#[test]
fn tfvars_attributes_are_definitions() {
    // A values file assigns root-module variables. The grammar is shared with .tf,
    // where a bare top-level attribute would be invalid, so this pattern only fires
    // on values files.
    let src = "region = \"eu-west-2\"\nreplicas = 3\n";
    let f = facts(Language::Hcl, src);

    let keys: Vec<&str> = f
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Key)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(keys, vec!["region", "replicas"], "got {keys:?}");

    let region = f.symbols.iter().find(|s| s.name == "region").unwrap();
    assert_eq!(region.name_span.text(src), "region");
    assert_eq!(region.full_span.text(src), "region = \"eu-west-2\"");
}

#[test]
fn tf_block_arguments_are_still_not_definitions() {
    // Only *top-level* attributes are values-file assignments; a provider argument
    // nested in a block is not a renameable address.
    let src = "resource \"aws_s3_bucket\" \"b\" {\n  bucket = \"x\"\n}\n";
    let f = facts(Language::Hcl, src);
    assert!(
        !f.symbols.iter().any(|s| s.name == "bucket"),
        "got {:?}",
        f.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}
