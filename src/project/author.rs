use super::{bounded_text, Project};
use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::model::SymbolKind;
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use crate::transpile::ir::{Item, Module};
use anyhow::{ensure, Context, Result};
use clap::{Args, Subcommand};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Print the bounded authoring manifest and transaction workflow.")]
    Guide,
    #[command(about = "Inspect the bounded semantic IR contract without scanning a project.")]
    SemanticSchema(super::semantic_ir::SchemaOptions),
    #[command(about = "Validate and canonically identify source-free semantic IR JSON.")]
    ValidateSemantic(ValidateSemanticOptions),
    #[command(about = "Apply a checked semantic change without scanning a project.")]
    ApplySemanticChange(super::semantic_change::ApplyOptions),
    #[command(
        about = "Replace one Rust, Go, Java, TypeScript or TSX function body, retaining surrounding source."
    )]
    ReplaceBody(ReplaceBodyOptions),
    #[command(about = "Replace one supported function body from source-free semantic IR JSON.")]
    ReplaceBodySemantic(ReplaceBodyOptions),
    #[command(about = "Apply a checked semantic delta to one supported function body.")]
    EditBodySemantic(ReplaceBodyOptions),
    #[command(
        about = "Replace one Rust function declaration while retaining its name and outer attributes."
    )]
    ReplaceDeclaration(ReplaceBodyOptions),
    #[command(
        about = "Insert one Rust function into a file, module, impl or trait, retaining existing source bytes."
    )]
    InsertDeclaration(ReplaceBodyOptions),
    #[command(about = "Plan disjoint authoring edits from one revision as one transaction.")]
    Batch(BatchOptions),
}

pub fn guide() -> Value {
    json!({
        "schema": "fr-author-guide-1",
        "purpose": "Bounded structural authoring through revision-bound project handles.",
        "limits": {
            "operations": {"minimum": 1, "maximum": 32},
            "manifest_bytes": 65536,
            "fragment_bytes": 65536,
            "diff_bytes": {"minimum": 0, "maximum": 65536}
        },
        "operations": [
            {"op": "replace-body", "requires": ["handle", "from"],
                "targets": "supported function or method body"},
            {"op": "replace-body-semantic", "requires": ["handle", "from"],
                "input-schema": "fr-semantic-body-1",
                "targets": "supported function or method body"},
            {"op": "edit-body-semantic", "requires": ["handle", "from"],
                "input-schema": "fr-semantic-change-1",
                "targets": "supported function or method body"},
            {"op": "replace-declaration", "requires": ["handle", "from"],
                "targets": "Rust function declaration with unchanged name"},
            {"op": "insert-declaration", "requires": ["handle", "from"],
                "targets": "Rust file, module, impl or trait"},
            {"op": "organize-imports", "requires": ["handle"],
                "targets": "supported source file"}
        ],
        "manifest": {
            "revision": "optional; required when any handle is a short ID",
            "operations": [{"op": "replace-body", "handle": "<FULL_HANDLE>",
                "from": "<FRAGMENT_PATH>"}],
            "postconditions": {"files-changed": "optional exact count", "edits": "optional exact count",
                "changed-operations": "optional exact count", "paths-changed": "optional exact ordered paths"}
        },
        "workflow": [
            {"step": "inspect", "command": "fr project select <SELECTOR>... --source --bytes <N>"},
            {"step": "preview", "command": "fr author batch --from <MANIFEST>"},
            {"step": "save", "command": "fr author batch --from <MANIFEST> --save-plan --plan-basis <PLAN_CONTEXT_BASIS>"},
            {"step": "checked-delivery", "command": "fr workflow --from <WORKFLOW_MANIFEST> --write"},
            {"step": "apply", "command": "fr history apply <TX> --write --context-basis <TRANSACTION_CONTEXT_BASIS>"},
            {"step": "patch", "command": "fr history patch <TX> --output <PATCH>"},
            {"step": "undo-preview", "command": "fr history undo <TX>"},
            {"step": "undo", "command": "fr history undo <TX> --write"},
            {"step": "redo-preview", "command": "fr history redo <TX>"},
            {"step": "redo", "command": "fr history redo <TX> --write --context-basis <TRANSACTION_CONTEXT_BASIS>"}
        ],
        "evidence": [
            "Review the complete preview before using its plan_context_basis.",
            "Use workflow delivery when reviewed project checks cover the requested lifecycle.",
            "Run declared checks on original, changed, undone and redone states.",
            "Retain refusals, coverage gaps, patch checks and receiver evidence."
        ]
    })
}

#[derive(Args)]
pub struct BatchOptions {
    #[arg(
        long,
        help = "JSON manifest of 1 through 32 authoring operations, at most 64 KiB."
    )]
    pub from: PathBuf,
    #[arg(
        long,
        default_value_t = 4096,
        help = "Maximum UTF-8 diff bytes, from 0 through 65536."
    )]
    pub diff_bytes: usize,
    #[arg(
        long,
        help = "Record and apply all edits after checking the source revision."
    )]
    pub write: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchManifest {
    pub(super) revision: Option<String>,
    pub(super) operations: Vec<BatchStep>,
    pub(super) postconditions: Option<BatchPostconditions>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct BatchPostconditions {
    pub(super) files_changed: Option<usize>,
    pub(super) edits: Option<usize>,
    pub(super) changed_operations: Option<usize>,
    pub(super) paths_changed: Option<Vec<PathBuf>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchStep {
    pub(super) op: BatchOperation,
    pub(super) handle: String,
    pub(super) from: Option<PathBuf>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BatchOperation {
    ReplaceBody,
    ReplaceBodySemantic,
    EditBodySemantic,
    ReplaceDeclaration,
    InsertDeclaration,
    OrganizeImports,
}

#[derive(Args)]
pub struct ReplaceBodyOptions {
    #[arg(help = "Full project handle, or a short ID with --revision.")]
    pub handle: String,
    #[arg(long, help = "Source revision required for a short ID.")]
    pub revision: Option<String>,
    #[arg(
        long,
        help = "UTF-8 fragment file, absolute or relative to the workspace root; at most 64 KiB."
    )]
    pub from: PathBuf,
    #[arg(
        long,
        default_value_t = 4096,
        help = "Maximum UTF-8 diff bytes, from 0 through 65536."
    )]
    pub diff_bytes: usize,
    #[arg(
        long,
        help = "Record and apply the edit after checking the source revision."
    )]
    pub write: bool,
}

#[derive(Args)]
pub struct ValidateSemanticOptions {
    #[arg(
        long,
        help = "Semantic JSON file, absolute or relative to the workspace root; at most 64 KiB."
    )]
    pub from: PathBuf,
    #[arg(long, help = "Include the canonical semantic payload in the report.")]
    pub canonical: bool,
}

pub struct Plan {
    pub edits: EditSet,
    pub report: Value,
}

pub fn semantic_body_admitted(
    schema_matches: bool,
    target_supported: bool,
    source_free: bool,
    bounded: bool,
) -> bool {
    schema_matches && target_supported && source_free && bounded
}

impl Plan {
    pub fn set_diff(&mut self, diff: &str, budget: usize) {
        self.report["diff"] = bounded_text(diff, budget);
    }
}

fn digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

struct BodySyntax {
    prefix: &'static str,
    suffix: &'static str,
    item: &'static str,
    blocks: &'static [&'static str],
    nested_item: bool,
    targets: &'static [&'static str],
    bindings: &'static [&'static str],
}

impl BodySyntax {
    fn for_language(language: Language) -> Result<Self> {
        match language {
            Language::Rust => Ok(Self {
                prefix: "fn __fr_body__() ",
                suffix: "",
                item: "function_item",
                blocks: &["block"],
                nested_item: false,
                targets: &["function_item"],
                bindings: &[],
            }),
            Language::Go => Ok(Self {
                prefix: "func __fr_body__() ",
                suffix: "",
                item: "function_declaration",
                blocks: &["block"],
                nested_item: false,
                targets: &["function_declaration", "method_declaration"],
                bindings: &[],
            }),
            Language::TypeScript | Language::Tsx => Ok(Self {
                prefix: "function __fr_body__() ",
                suffix: "",
                item: "function_declaration",
                blocks: &["statement_block"],
                nested_item: false,
                targets: &[
                    "function_declaration",
                    "generator_function_declaration",
                    "method_definition",
                ],
                bindings: &["variable_declarator", "public_field_definition"],
            }),
            Language::Java => Ok(Self {
                prefix: "class __FrBody__ { void __fr_body__() ",
                suffix: "}",
                item: "method_declaration",
                blocks: &["block", "constructor_body"],
                nested_item: true,
                targets: &["method_declaration", "constructor_declaration"],
                bindings: &[],
            }),
            _ => anyhow::bail!(
                "body replacement supports Rust, Go, Java, TypeScript and TSX; select a supported function."
            ),
        }
    }

    fn is_block(&self, node: tree_sitter::Node<'_>) -> bool {
        self.blocks.contains(&node.kind())
    }

    fn wrapped_item<'tree>(&self, parsed: &'tree Parsed) -> Option<tree_sitter::Node<'tree>> {
        let outer = parsed.root().named_child(0)?;
        if !self.nested_item {
            return Some(outer);
        }
        let body = outer.child_by_field_name("body")?;
        let mut cursor = body.walk();
        let item = body
            .named_children(&mut cursor)
            .find(|node| node.kind() == self.item);
        item
    }

    fn block_span(&self, body: tree_sitter::Node<'_>) -> Result<Span> {
        ensure!(self.is_block(body), "selected function needs a block body.");
        let mut cursor = body.walk();
        let mut braces = body
            .children(&mut cursor)
            .filter(|child| matches!(child.kind(), "{" | "}"));
        let open = braces.next().context("body needs an opening brace.")?;
        let close = braces.next().context("body needs a closing brace.")?;
        ensure!(
            open.kind() == "{"
                && close.kind() == "}"
                && !open.is_missing()
                && !close.is_missing()
                && braces.next().is_none(),
            "body needs exactly one pair of outer braces."
        );
        Ok(Span::new(open.start_byte(), close.end_byte()))
    }
}

fn fragment(path: &Path) -> Result<String> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "replacement input must be a regular file."
    );
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "replacement input must be a regular file."
    );
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "replacement input exceeds 64 KiB.");
    let text = String::from_utf8(bytes).context("replacement input must use UTF-8.")?;
    ensure!(
        !text.contains('\0'),
        "replacement input contains a NUL byte."
    );
    Ok(text.trim().to_owned())
}

fn replacement(
    path: &Path,
    language: Language,
    syntax: &BodySyntax,
    allow_expression: bool,
) -> Result<(String, &'static str)> {
    let text = fragment(path)?;
    let prefix = syntax.prefix;
    let wrapped = format!("{prefix}{text}{}", syntax.suffix);
    let parsed = Parsers::new().parse(language, &wrapped)?;
    if let Some(item) = syntax.wrapped_item(&parsed) {
        if let Some(body) = item.child_by_field_name("body") {
            if let Ok(span) = syntax.block_span(body) {
                if !parsed.has_errors()
                    && parsed.root().named_child_count() == 1
                    && item.kind() == syntax.item
                    && span.start == prefix.len()
                    && span.end == prefix.len() + text.len()
                {
                    return Ok((text, "block"));
                }
            }
        }
    }
    if allow_expression && matches!(language, Language::TypeScript | Language::Tsx) {
        let prefix = "const __fr_body__ = () => ";
        let wrapped = format!("{prefix}{text}");
        let parsed = Parsers::new().parse(language, &wrapped)?;
        let declaration = parsed.root().named_child(0);
        let declarator = declaration.and_then(|node| {
            let mut cursor = node.walk();
            let found = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "variable_declarator");
            found
        });
        let arrow = declarator.and_then(|node| node.child_by_field_name("value"));
        let body = arrow.and_then(|node| node.child_by_field_name("body"));
        if !parsed.has_errors()
            && parsed.root().named_child_count() == 1
            && arrow.is_some_and(|node| node.kind() == "arrow_function")
            && body.is_some_and(|node| {
                !syntax.is_block(node)
                    && node.start_byte() == prefix.len()
                    && node.end_byte() == wrapped.len()
            })
        {
            return Ok((text, "expression"));
        }
    }
    anyhow::bail!(if allow_expression {
        "replacement must contain one complete block or arrow expression in the selected language."
    } else {
        "replacement must contain exactly one complete block in the selected language."
    })
}

pub fn validate_semantic(root: &Path, options: &ValidateSemanticOptions) -> Result<Value> {
    let input = fragment(&root.join(&options.from))?;
    let validated = super::semantic_change::validate_body_input(&input)?;
    let mut report = json!({
        "schema": "fr-semantic-validation-1",
        "semantic_schema": super::semantic_ir::BODY_SCHEMA,
        "valid": true,
        "input_sha256": validated.input_sha256,
        "canonical_sha256": digest(&validated.canonical),
        "statements": validated.manifest.body.len(),
        "semantic_nodes": validated.nodes,
        "source_free": true,
    });
    if options.canonical {
        report["canonical"] = serde_json::from_str(&validated.canonical)?;
    }
    Ok(report)
}

fn outer_callable_bodies<'tree>(
    node: tree_sitter::Node<'tree>,
    syntax: &BodySyntax,
    found: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if syntax.targets.contains(&node.kind()) {
        if let Some(body) = node.child_by_field_name("body") {
            found.push(body);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        outer_callable_bodies(child, syntax, found);
    }
}

fn function_initializer(mut value: tree_sitter::Node<'_>) -> Result<tree_sitter::Node<'_>> {
    loop {
        match value.kind() {
            "arrow_function" | "function_expression" | "generator_function" => return Ok(value),
            "parenthesized_expression" | "as_expression" | "satisfies_expression"
            | "non_null_expression" | "type_assertion" => {
                let mut cursor = value.walk();
                let mut children = value
                    .named_children(&mut cursor)
                    .filter(|child| !child.is_extra());
                let operand = if value.kind() == "type_assertion" {
                    children.nth(1)
                } else {
                    children.next()
                };
                value = operand.context("initializer wrapper has no expression operand.")?;
            }
            _ => anyhow::bail!(
                "select an arrow or function initializer, optionally inside parentheses or type-only assertions."
            ),
        }
    }
}

fn function_fragment<'tree>(
    parsed: &'tree Parsed,
    text: &str,
    allow_documentation: bool,
    allow_bodyless: bool,
) -> Result<tree_sitter::Node<'tree>> {
    let mut cursor = parsed.root().walk();
    let mut items = parsed.root().named_children(&mut cursor);
    let mut item = items
        .next()
        .context("fragment needs one Rust function declaration.")?;
    let start = item.start_byte();
    while allow_documentation
        && matches!(item.kind(), "line_comment" | "block_comment")
        && item.child_by_field_name("outer").is_some()
    {
        item = items
            .next()
            .context("documentation needs a following Rust function.")?;
    }
    let supported = item.kind() == "function_item" && item.child_by_field_name("body").is_some()
        || allow_bodyless && item.kind() == "function_signature_item";
    ensure!(
        !parsed.has_errors()
            && items.next().is_none()
            && start == 0
            && supported
            && item.end_byte() == text.len(),
        if allow_bodyless {
            "fragment requires one Rust function or bodyless trait function; only insertion accepts leading outer documentation comments."
        } else {
            "fragment requires one Rust function with a body; only insertion accepts leading outer documentation comments."
        }
    );
    Ok(item)
}

impl Project<'_> {
    pub fn author_batch(&self, options: &BatchOptions) -> Result<Plan> {
        let manifest: BatchManifest =
            serde_json::from_str(&fragment(&self.root.join(&options.from))?)
                .context("batch input must be an authoring manifest.")?;
        self.author_batch_manifest(manifest, options.diff_bytes)
    }

    pub(super) fn author_batch_manifest(
        &self,
        manifest: BatchManifest,
        diff_bytes: usize,
    ) -> Result<Plan> {
        ensure!(
            diff_bytes <= 65536,
            "diff bytes must be between 0 and 65536."
        );
        ensure!(
            (1..=32).contains(&manifest.operations.len()),
            "batch needs 1 through 32 operations."
        );
        if let Some(revision) = &manifest.revision {
            ensure!(
                revision == &self.revision || revision == &self.revision[..32],
                "stale revision; obtain a fresh project map."
            );
        }
        let mut edits = EditSet::new();
        let mut regions: Vec<(PathBuf, Span)> = Vec::new();
        let mut steps = Vec::new();
        let mut changed_operations = 0usize;
        for (index, step) in manifest.operations.into_iter().enumerate() {
            let revision = (!step.handle.starts_with("frp1:"))
                .then(|| manifest.revision.clone())
                .flatten();
            let plan = match step.op {
                BatchOperation::OrganizeImports => {
                    ensure!(
                        step.from.is_none(),
                        "organize-imports does not accept a fragment path."
                    );
                    self.organize_imports(&step.handle, revision.as_deref())
                }
                operation => {
                    let operation_options = ReplaceBodyOptions {
                        revision: revision.clone(),
                        handle: step.handle.clone(),
                        from: step
                            .from
                            .context("authoring operation requires a fragment path.")?,
                        diff_bytes,
                        write: false,
                    };
                    match operation {
                        BatchOperation::ReplaceBody => self.replace_body(&operation_options),
                        BatchOperation::ReplaceBodySemantic => {
                            self.replace_body_semantic(&operation_options)
                        }
                        BatchOperation::EditBodySemantic => {
                            self.edit_body_semantic(&operation_options)
                        }
                        BatchOperation::ReplaceDeclaration => {
                            self.replace_declaration(&operation_options)
                        }
                        BatchOperation::InsertDeclaration => {
                            self.insert_declaration(&operation_options)
                        }
                        BatchOperation::OrganizeImports => unreachable!(),
                    }
                }
            }
            .with_context(|| format!("batch operation {} failed", index + 1))?;
            if matches!(step.op, BatchOperation::OrganizeImports) {
                ensure!(
                    !plan.edits.is_empty(),
                    "organize-imports produced no change; omit this batch step."
                );
            }
            changed_operations += usize::from(plan.report["changed"] == true);
            let handle = self.explicit_handle(&step.handle, revision.as_deref())?;
            let id = self.resolve_handle(&handle)?;
            let path = self.root.join(&self.nodes[id].path);
            let key = match step.op {
                BatchOperation::ReplaceBody
                | BatchOperation::ReplaceBodySemantic
                | BatchOperation::EditBodySemantic => "body",
                BatchOperation::ReplaceDeclaration => "declaration",
                BatchOperation::InsertDeclaration => "insertion",
                BatchOperation::OrganizeImports => "imports",
            };
            let span: Span = serde_json::from_value(plan.report[key]["before_span"].clone())?;
            for (previous, selected) in &regions {
                let conflict = super::author_selection_conflict(
                    selected.start,
                    selected.end,
                    span.start,
                    span.end,
                );
                ensure!(previous != &path || !conflict,
                    "batch selections overlap or share an insertion boundary; use disjoint selections.");
            }
            let mut summary = if matches!(step.op, BatchOperation::OrganizeImports) {
                json!({"operation": plan.report["query"], "handle": plan.report["handle"],
                    "path": plan.report["path"], "before_span": span,
                    "imports": plan.report["imports"], "changed": plan.report["changed"],
                    "preservation": plan.report["preservation"]})
            } else {
                let before = span.text(&self.sources[&path]);
                let after = plan
                    .edits
                    .edits_for(&path)
                    .and_then(|items| items.first())
                    .map_or(before, |edit| edit.replacement.as_str());
                json!({"operation": plan.report["query"], "handle": plan.report["handle"],
                    "path": plan.report["path"], "before_span": span, "before_bytes": before.len(),
                    "after_bytes": after.len(), "before_sha256": digest(before), "after_sha256": digest(after),
                    "signature": plan.report["signature"], "changed": plan.report["changed"],
                    "preservation": plan.report["preservation"]})
            };
            for key in [
                "replacement_signature",
                "name_check",
                "name_resolution_checked",
                "semantic_input",
                "semantic_render",
                "semantic_change",
            ] {
                if let Some(value) = plan.report.get(key) {
                    summary[key] = value.clone();
                }
            }
            steps.push(summary);
            regions.push((path, span));
            edits.extend(plan.edits);
        }
        for (path, replacements) in edits.iter() {
            let language = self
                .index
                .file(path)
                .context("batch file is not indexed.")?
                .language;
            let updated = crate::edit::apply_to_string(&self.sources[path], replacements)?;
            ensure!(
                !Parsers::new().parse(language, &updated)?.has_errors(),
                "combined batch introduces parser errors in its destination context."
            );
        }
        let mut report = self.envelope("batch");
        report["schema"] = json!("fr-author-batch-1");
        report["span_basis"] = json!("original-source");
        report["files_changed"] = json!(edits.file_count());
        report["changed"] = json!(!edits.is_empty());
        report["steps"] = json!(steps);
        let mut postconditions = Vec::new();
        if let Some(expected) = manifest.postconditions {
            ensure!(
                expected.files_changed.is_some()
                    || expected.edits.is_some()
                    || expected.changed_operations.is_some()
                    || expected.paths_changed.is_some(),
                "postconditions must declare at least one expected outcome."
            );
            let mut check = |name: &str, expected: Value, actual: Value| -> Result<()> {
                let held = expected == actual;
                postconditions.push(json!({"postcondition": name, "expected": expected,
                    "actual": actual, "held": held}));
                ensure!(
                    held,
                    "postcondition {name} failed: expected {}, actual {}.",
                    expected,
                    actual
                );
                Ok(())
            };
            if let Some(expected) = expected.files_changed {
                check("files-changed", json!(expected), json!(edits.file_count()))?;
            }
            if let Some(expected) = expected.edits {
                check("edits", json!(expected), json!(edits.edit_count()))?;
            }
            if let Some(expected) = expected.changed_operations {
                check(
                    "changed-operations",
                    json!(expected),
                    json!(changed_operations),
                )?;
            }
            if let Some(mut expected) = expected.paths_changed {
                ensure!(
                    expected.iter().all(|path| {
                        path.is_relative()
                            && path
                                .components()
                                .all(|part| matches!(part, std::path::Component::Normal(_)))
                    }),
                    "paths-changed entries must be normalized relative paths."
                );
                expected.sort();
                expected.dedup();
                let actual = edits
                    .paths()
                    .filter(|path| edits.edits_for(path).is_some_and(|items| !items.is_empty()))
                    .map(|path| path.strip_prefix(&self.root).unwrap_or(path).to_path_buf())
                    .collect::<Vec<_>>();
                check("paths-changed", json!(expected), json!(actual))?;
            }
        }
        report["postconditions"] = json!(postconditions);
        report["postconditions_held"] = json!(true);
        report["validation"] = json!("reparse-strict");
        report["behavior_checked"] = json!(false);
        report["atomic_snapshot"] = json!(false);
        Ok(Plan { edits, report })
    }

    fn organize_imports(&self, selected: &str, revision: Option<&str>) -> Result<Plan> {
        let handle = self.explicit_handle(selected, revision)?;
        let id = self.resolve_handle(&handle)?;
        ensure!(
            self.nodes[id].kind == "file",
            "organize-imports requires a file handle."
        );
        let path = self.root.join(&self.nodes[id].path);
        let source = &self.sources[&path];
        let plan = crate::refactor::imports::plan_in(self.index, &path, source)?;
        let import_edits = plan.edits.edits_for(&path).unwrap_or(&[]);
        let before_span = import_edits
            .iter()
            .fold(None, |region: Option<Span>, edit| {
                Some(region.map_or(edit.span, |region| {
                    Span::new(
                        region.start.min(edit.span.start),
                        region.end.max(edit.span.end),
                    )
                }))
            });
        let edit_reports = import_edits
            .iter()
            .map(|edit| {
                let before = edit.span.text(source);
                json!({"before_span": edit.span, "before_bytes": before.len(),
                    "after_bytes": edit.replacement.len(), "before_sha256": digest(before),
                    "after_sha256": digest(&edit.replacement), "reason": edit.reason})
            })
            .collect::<Vec<_>>();
        let removed = plan
            .removed
            .iter()
            .map(|item| {
                json!({"path": item.path, "bindings": item.bindings,
                "span": item.span, "line": item.line})
            })
            .collect::<Vec<_>>();
        let kept = plan
            .warnings
            .iter()
            .map(|warning| {
                json!({"line": warning.line, "col": warning.col,
                "reason": warning.detail})
            })
            .collect::<Vec<_>>();
        let mut report = self.envelope("organize-imports");
        report["schema"] = json!("fr-author-1");
        report["handle"] = json!(self.handle(id));
        report["path"] = bounded_text(&self.nodes[id].path.to_string_lossy(), 512);
        report["imports"] = json!({"before_span": before_span.unwrap_or(Span::new(0, source.len())),
            "edits": edit_reports, "removed": removed, "kept": kept,
            "sorted_blocks": plan.sorted_blocks});
        report["changed"] = json!(!plan.edits.is_empty());
        report["validation"] = json!("reparse-strict");
        report["preservation"] = json!("bytes outside the reported import edit spans");
        report["behavior_checked"] = json!(false);
        report["atomic_snapshot"] = json!(false);
        Ok(Plan {
            edits: plan.edits,
            report,
        })
    }

    pub fn insert_declaration(&self, options: &ReplaceBodyOptions) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65536,
            "diff bytes must be between 0 and 65536."
        );
        let handle = self.explicit_handle(&options.handle, options.revision.as_deref())?;
        let id = self.resolve_handle(&handle)?;
        let is_file = self.nodes[id].kind == "file";
        let path = self.root.join(&self.nodes[id].path);
        let info = self
            .index
            .file(&path)
            .context("selected file is not indexed.")?;
        ensure!(
            info.language == Language::Rust,
            "declaration insertion supports Rust files only."
        );
        let source = &self.sources[&path];
        let parsed = Parsers::new().parse(Language::Rust, source)?;
        ensure!(
            !parsed.has_errors(),
            "declaration insertion requires a file without parser errors."
        );
        let (container, offset, container_kind, container_name) = if is_file {
            (parsed.root(), source.len(), "file", None)
        } else {
            let symbol = self.nodes[id]
                .symbol
                .and_then(|id| self.index.symbol(id))
                .context(
                    "declaration insertion requires a Rust file, module, trait or method handle.",
                )?;
            let mut selected = parsed
                .root()
                .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end);
            let declaration = loop {
                let node = selected.context(
                    "select a Rust file, inline module, trait, or a direct impl or trait method.",
                )?;
                let names_symbol = node
                    .child_by_field_name("name")
                    .is_some_and(|name| Span::from(name) == symbol.name_span);
                if symbol.kind == SymbolKind::Module && node.kind() == "mod_item" && names_symbol {
                    break node;
                }
                if symbol.kind == SymbolKind::Trait && node.kind() == "trait_item" && names_symbol {
                    break node;
                }
                if symbol.kind == SymbolKind::Method
                    && matches!(node.kind(), "function_item" | "function_signature_item")
                    && names_symbol
                {
                    let list = node
                        .parent()
                        .context("selected method has no declaration list.")?;
                    ensure!(
                        list.kind() == "declaration_list",
                        "select a direct impl or trait method, not a nested function."
                    );
                    let owner = list
                        .parent()
                        .context("selected method has no enclosing declaration.")?;
                    ensure!(
                        matches!(owner.kind(), "impl_item" | "trait_item"),
                        "select a direct impl or trait method."
                    );
                    break owner;
                }
                selected = node.parent();
            };
            let body = declaration.child_by_field_name("body").with_context(|| {
                if declaration.kind() == "mod_item" {
                    "external module declarations have no inline body; select their source file instead."
                } else {
                    "selected declaration has no insertable body."
                }
            })?;
            ensure!(
                body.kind() == "declaration_list",
                "selected declaration requires a braced declaration list."
            );
            let mut cursor = body.walk();
            let close = body
                .children(&mut cursor)
                .find(|child| child.kind() == "}" && !child.is_missing())
                .context("declaration body needs a closing brace.")?;
            let before = &source[..close.start_byte()];
            let offset = super::declaration_insertion_offset(before, body.start_byte());
            let kind = match declaration.kind() {
                "mod_item" => "inline-module",
                "impl_item" => "impl",
                "trait_item" => "trait",
                _ => unreachable!(),
            };
            let name = if declaration.kind() == "impl_item" {
                declaration.child_by_field_name("type")
            } else {
                declaration.child_by_field_name("name")
            }
            .map(|name| Span::from(name).text(source).to_owned());
            (body, offset, kind, name)
        };
        let text = fragment(&self.root.join(&options.from))?;
        let fragment_tree = Parsers::new().parse(Language::Rust, &text)?;
        let function = function_fragment(&fragment_tree, &text, true, container_kind == "trait")?;
        let name = Span::from(
            function
                .child_by_field_name("name")
                .context("function needs a name.")?,
        )
        .text(&text);
        let normalized = name.strip_prefix("r#").unwrap_or(name);
        let mut pending_outer = false;
        let mut cursor = container.walk();
        for item in container.named_children(&mut cursor) {
            if let Some(existing) = item.child_by_field_name("name") {
                let existing = Span::from(existing).text(source);
                ensure!(
                    existing.strip_prefix("r#").unwrap_or(existing) != normalized,
                    "a direct item already has this name; use replacement or choose a new name."
                );
            }
            match item.kind() {
                "attribute_item" => pending_outer = true,
                "line_comment" | "block_comment" => {
                    pending_outer |= item.child_by_field_name("outer").is_some();
                }
                "inner_attribute_item" => {}
                _ => pending_outer = false,
            }
        }
        ensure!(!pending_outer, "trailing outer attributes or documentation could attach to the insertion; resolve them first.");
        let newline = if source
            .find('\n')
            .is_some_and(|at| at > 0 && source.as_bytes()[at - 1] == b'\r')
        {
            "\r\n"
        } else {
            "\n"
        };
        let leading = if offset == 0 || source[..offset].ends_with('\n') {
            ""
        } else {
            newline
        };
        let inserted = format!("{leading}{text}{newline}");
        let span = Span::new(offset, offset);
        let mut edits = EditSet::new();
        edits.add(
            &path,
            Edit::new(span, &inserted, "Append the new function declaration."),
        );
        let updated = crate::edit::apply_to_string(source, edits.edits_for(&path).unwrap_or(&[]))?;
        ensure!(
            !Parsers::new().parse(Language::Rust, &updated)?.has_errors(),
            "insertion introduces parser errors in its destination context."
        );
        let body = function.child_by_field_name("body");
        ensure!(
            body.is_some() || container_kind == "trait",
            "only trait containers accept bodyless functions."
        );
        let signature_end = body.map_or(function.end_byte(), |body| body.start_byte());
        let mut report = self.envelope("insert-declaration");
        report["schema"] = json!("fr-author-1");
        report["handle"] = json!(self.handle(id));
        report["path"] = bounded_text(&self.nodes[id].path.to_string_lossy(), 512);
        report["signature"] = json!({"basis": "syntax-header", "text": bounded_text(text[function.start_byte()..signature_end].trim_end(), 512)});
        report["declaration"] = json!({"name": bounded_text(name, 512), "kind": if body.is_some() { "function" } else { "function-signature" }, "span": Span::new(offset+leading.len(), offset+leading.len()+text.len()), "bytes": text.len(), "sha256": digest(&text)});
        if function.start_byte() > 0 {
            let documentation = &text[..function.start_byte()];
            let start = offset + leading.len();
            report["documentation"] = json!({"kind": "outer-doc-comments", "span": Span::new(start, start + documentation.len()), "bytes": documentation.len(), "sha256": digest(documentation)});
        }
        report["insertion"] = json!({"before_span": span, "after_span": Span::new(span.start, span.start+inserted.len()), "added_bytes": inserted.len(), "leading_separator": leading, "trailing_separator": newline, "sha256": digest(&inserted)});
        report["name_check"] = json!(match container_kind {
            "file" => "direct top-level item names; Rust namespaces are not distinguished.",
            "inline-module" =>
                "direct items in the selected module; Rust namespaces are not distinguished.",
            "impl" => "direct items in the selected impl; Rust namespaces are not distinguished.",
            "trait" => "direct items in the selected trait; Rust namespaces are not distinguished.",
            _ => unreachable!(),
        });
        if !is_file {
            report["container"] = json!({"kind": container_kind, "name": bounded_text(container_name.as_deref().unwrap_or(&self.nodes[id].name), 512),
                "before_span": Span::from(container)});
        }
        report["name_resolution_checked"] = json!(false);
        report["changed"] = json!(true);
        report["validation"] = json!("reparse-strict");
        report["preservation"] = json!("all existing source bytes");
        report["behavior_checked"] = json!(false);
        report["atomic_snapshot"] = json!(false);
        Ok(Plan { edits, report })
    }

    pub fn replace_declaration(&self, options: &ReplaceBodyOptions) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65536,
            "diff bytes must be between 0 and 65536."
        );
        let handle = self.explicit_handle(&options.handle, options.revision.as_deref())?;
        let id = self.resolve_handle(&handle)?;
        let symbol = self.nodes[id]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("declaration replacement requires a Rust function handle.")?;
        ensure!(
            symbol.language == Language::Rust,
            "declaration replacement supports Rust functions only."
        );
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(Language::Rust, source)?;
        ensure!(
            !parsed.has_errors(),
            "declaration replacement requires a file without parser errors."
        );
        let mut selected = parsed
            .root()
            .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end);
        let function = loop {
            let node = selected.context("select a Rust function declaration with a body.")?;
            if node.kind() == "function_item" {
                ensure!(
                    node.child_by_field_name("name")
                        .is_some_and(|name| Span::from(name) == symbol.name_span),
                    "selected handle does not name this function."
                );
                break node;
            }
            selected = node.parent();
        };
        let span = Span::from(function);
        let before = &source[span.start..span.end];
        let after = fragment(&self.root.join(&options.from))?;
        ensure!(
            super::body_replacement_budget(before.len(), after.len()),
            "old and new declarations must each fit 2 through 65536 bytes."
        );
        let replacement = Parsers::new().parse(Language::Rust, &after)?;
        let item = function_fragment(&replacement, &after, false, false)?;
        let name = item
            .child_by_field_name("name")
            .context("replacement function needs a name.")?;
        ensure!(
            Span::from(name).text(&after) == symbol.name_span.text(source),
            "replacement must retain the function name; use rename to update callers."
        );
        let body = item
            .child_by_field_name("body")
            .context("replacement function needs a body.")?;
        let mut edits = EditSet::new();
        if before != after {
            edits.add(
                &symbol.file,
                Edit::new(span, &after, "Replace the selected function declaration."),
            );
        }
        let updated =
            crate::edit::apply_to_string(source, edits.edits_for(&symbol.file).unwrap_or(&[]))?;
        ensure!(
            !Parsers::new().parse(Language::Rust, &updated)?.has_errors(),
            "replacement introduces parser errors in its destination context."
        );
        let mut report = self.envelope("replace-declaration");
        report["schema"] = json!("fr-author-1");
        report["handle"] = json!(self.handle(id));
        report["path"] = bounded_text(&self.nodes[id].path.to_string_lossy(), 512);
        report["signature"] = self.signature(id)?;
        report["replacement_signature"] = json!({"basis": "syntax-header", "text": bounded_text(after[..body.start_byte()].trim_end(), 512)});
        report["declaration"] = json!({"before_span":span,"after_span":Span::new(span.start,span.start+after.len()),
            "before_bytes":before.len(),"after_bytes":after.len(),"before_sha256":digest(before),"after_sha256":digest(&after)});
        report["changed"] = json!(before != after);
        report["validation"] = json!("reparse-strict");
        report["preservation"] = json!("function name and bytes outside the selected declaration");
        report["behavior_checked"] = json!(false);
        report["atomic_snapshot"] = json!(false);
        Ok(Plan { edits, report })
    }

    pub fn replace_body(&self, options: &ReplaceBodyOptions) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65536,
            "diff bytes must be between 0 and 65536."
        );
        let handle = self.explicit_handle(&options.handle, options.revision.as_deref())?;
        let id = self.resolve_handle(&handle)?;
        let symbol = self.nodes[id]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("body replacement requires a function handle.")?;
        let language = symbol.language;
        let syntax = BodySyntax::for_language(language)?;
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(language, source)?;
        ensure!(
            !parsed.has_errors(),
            "body replacement requires a file without parser errors."
        );
        let mut selected = parsed
            .root()
            .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end);
        let mut binding_start = None;
        let function = loop {
            let node = selected
                .context("select a function declaration, method or supported function binding.")?;
            let binding = syntax.bindings.contains(&node.kind());
            if binding || syntax.targets.contains(&node.kind()) {
                ensure!(
                    node.child_by_field_name("name")
                        .is_some_and(|name| Span::from(name) == symbol.name_span),
                    "selected handle does not name this function."
                );
                if binding {
                    let value = node
                        .child_by_field_name("value")
                        .context("selected binding has no initializer.")?;
                    binding_start = Some(node.start_byte());
                    break function_initializer(value)?;
                }
                break node;
            }
            selected = node.parent();
        };
        let body = function
            .child_by_field_name("body")
            .context("selected function has no body.")?;
        let expression_arrow = matches!(language, Language::TypeScript | Language::Tsx)
            && function.kind() == "arrow_function"
            && !syntax.is_block(body);
        let (span, before_kind) = if syntax.is_block(body) {
            (syntax.block_span(body)?, "block")
        } else if expression_arrow {
            (Span::from(body), "expression")
        } else {
            anyhow::bail!("selected function needs a block body or an expression-bodied arrow.");
        };
        let before = &source[span.start..span.end];
        let (after, after_kind) = replacement(
            &self.root.join(&options.from),
            language,
            &syntax,
            function.kind() == "arrow_function",
        )?;
        ensure!(
            super::body_replacement_budget(before.len(), after.len()),
            "old and new bodies must each fit 1 through 65536 bytes."
        );
        let mut edits = EditSet::new();
        if before != after {
            edits.add(
                &symbol.file,
                Edit::new(span, &after, "Replace the selected function body."),
            );
        }
        let updated =
            crate::edit::apply_to_string(source, edits.edits_for(&symbol.file).unwrap_or(&[]))?;
        ensure!(
            !Parsers::new().parse(language, &updated)?.has_errors(),
            "replacement introduces parser errors in its destination context."
        );
        let mut report = self.envelope("replace-body");
        report["schema"] = json!("fr-author-1");
        report["handle"] = json!(self.handle(id));
        report["path"] = bounded_text(&self.nodes[id].path.to_string_lossy(), 512);
        report["signature"] = if let Some(start) = binding_start {
            json!({"basis": "syntax-header", "text": bounded_text(source[start..span.start].trim_end(), 512)})
        } else {
            self.signature(id)?
        };
        report["body"] = json!({"before_span":span,"after_span":Span::new(span.start,span.start+after.len()),
            "before_bytes":before.len(),"after_bytes":after.len(),"before_sha256":digest(before),"after_sha256":digest(&after)});
        report["body"]["before_kind"] = json!(before_kind);
        report["body"]["after_kind"] = json!(after_kind);
        report["changed"] = json!(before != after);
        report["validation"] = json!("reparse-strict");
        report["preservation"] = json!("bytes outside the selected body");
        report["behavior_checked"] = json!(false);
        report["atomic_snapshot"] = json!(false);
        Ok(Plan { edits, report })
    }

    pub fn replace_body_semantic(&self, options: &ReplaceBodyOptions) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65536,
            "diff bytes must be between 0 and 65536."
        );
        let input = fragment(&self.root.join(&options.from))?;
        let validated = super::semantic_change::validate_body_input(&input)?;
        let nodes = validated.nodes;
        let manifest = validated.manifest;
        let schema_matches = true;
        let source_free = true;
        let bounded = true;

        let handle = self.explicit_handle(&options.handle, options.revision.as_deref())?;
        let id = self.resolve_handle(&handle)?;
        let symbol = self.nodes[id]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("semantic body replacement requires a function handle.")?;
        let target_supported = matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method)
            && BodySyntax::for_language(symbol.language).is_ok();
        ensure!(
            semantic_body_admitted(schema_matches, target_supported, source_free, bounded),
            "selected target does not support semantic body replacement."
        );

        let language = symbol.language;
        let syntax = BodySyntax::for_language(language)?;
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(language, source)?;
        ensure!(
            !parsed.has_errors(),
            "semantic body replacement requires a file without parser errors."
        );
        let context = crate::transpile::read_module(language, source, parsed.root())?;
        let mut function =
            super::semantic::selected_function(&context, symbol).with_context(|| {
                format!(
                    "the {} '{}' has no exact semantic function model.",
                    symbol.kind.as_str(),
                    symbol.name
                )
            })?;
        function.body = manifest.body;
        let generated = Module {
            doc: Vec::new(),
            name: context.name.clone(),
            items: vec![Item::Function(function)],
            sweep_notes: Vec::new(),
        };
        let (rendered, fidelity) =
            crate::transpile::write_module_in_preserving_fields(language, &generated, &context)?;
        ensure!(
            fidelity.carried_verbatim == 0,
            "writer cannot render semantic body without carrying source verbatim."
        );
        let rendered_parse = Parsers::new().parse(language, &rendered)?;
        ensure!(
            !rendered_parse.has_errors(),
            "semantic body writer produced parser errors."
        );
        let mut bodies = Vec::new();
        outer_callable_bodies(rendered_parse.root(), &syntax, &mut bodies);
        ensure!(
            bodies.len() == 1,
            "semantic body rendering must produce exactly one outer function."
        );
        let body_span = syntax.block_span(bodies[0])?;
        let rendered_body = body_span.text(&rendered).to_owned();
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
        temporary.write_all(rendered_body.as_bytes())?;
        temporary.flush()?;
        let mut plan = self.replace_body(&ReplaceBodyOptions {
            handle: options.handle.clone(),
            revision: options.revision.clone(),
            from: temporary.path().to_path_buf(),
            diff_bytes: options.diff_bytes,
            write: false,
        })?;
        plan.report["query"] = json!("replace-body-semantic");
        plan.report["semantic_input"] = json!({
            "schema": "fr-semantic-body-1",
            "sha256": digest(&input),
            "semantic_nodes": nodes,
            "source_free": true
        });
        plan.report["semantic_render"] = json!({
            "language": language,
            "body_sha256": digest(&rendered_body),
            "fidelity": fidelity
        });
        Ok(plan)
    }

    pub fn edit_body_semantic(&self, options: &ReplaceBodyOptions) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65536,
            "diff bytes must be between 0 and 65536."
        );
        let change_input = fragment(&self.root.join(&options.from))?;
        let handle = self.explicit_handle(&options.handle, options.revision.as_deref())?;
        let id = self.resolve_handle(&handle)?;
        let symbol = self.nodes[id]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("semantic body editing requires a function handle.")?;
        let target_supported = matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method)
            && BodySyntax::for_language(symbol.language).is_ok();
        ensure!(
            target_supported,
            "selected target does not support semantic body editing."
        );
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(symbol.language, source)?;
        ensure!(
            !parsed.has_errors(),
            "semantic body editing requires a file without parser errors."
        );
        let context = crate::transpile::read_module(symbol.language, source, parsed.root())?;
        let function = super::semantic::selected_function(&context, symbol).with_context(|| {
            format!(
                "the {} '{}' has no exact semantic function model.",
                symbol.kind.as_str(),
                symbol.name
            )
        })?;
        let current = super::semantic_change::SemanticBody {
            schema: super::semantic_ir::BODY_SCHEMA.into(),
            body: function.body,
        };
        let body_input = serde_json::to_string(&current)?;
        let applied = super::semantic_change::apply(&body_input, &change_input)?;
        let result = serde_json::to_string(&applied.body)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
        temporary.write_all(result.as_bytes())?;
        temporary.flush()?;
        let mut plan = self.replace_body_semantic(&ReplaceBodyOptions {
            handle: options.handle.clone(),
            revision: options.revision.clone(),
            from: temporary.path().to_path_buf(),
            diff_bytes: options.diff_bytes,
            write: false,
        })?;
        plan.report["query"] = json!("edit-body-semantic");
        plan.report["semantic_change"] = json!({
            "schema":super::semantic_change::CHANGE_SCHEMA,
            "sha256":applied.change_sha256,
            "input_basis":applied.input_basis,
            "result_basis":applied.result_basis,
            "operations":applied.operations,
            "semantic_nodes":applied.nodes,
            "source_free":true
        });
        Ok(plan)
    }
}
