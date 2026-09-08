use super::{bounded_text, Project};
use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::{ensure, Context, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    #[command(
        about = "Replace one Rust, TypeScript or TSX function body, retaining surrounding source."
    )]
    ReplaceBody(ReplaceBodyOptions),
    #[command(
        about = "Replace one Rust function declaration while retaining its name and outer attributes."
    )]
    ReplaceDeclaration(ReplaceBodyOptions),
    #[command(
        about = "Insert one Rust function into a file or inline module, retaining existing source bytes."
    )]
    InsertDeclaration(ReplaceBodyOptions),
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

pub struct Plan {
    pub edits: EditSet,
    pub report: Value,
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
    item: &'static str,
    block: &'static str,
    targets: &'static [&'static str],
    bindings: &'static [&'static str],
}

impl BodySyntax {
    fn for_language(language: Language) -> Result<Self> {
        match language {
            Language::Rust => Ok(Self {
                prefix: "fn __fr_body__() ",
                item: "function_item",
                block: "block",
                targets: &["function_item"],
                bindings: &[],
            }),
            Language::TypeScript | Language::Tsx => Ok(Self {
                prefix: "function __fr_body__() ",
                item: "function_declaration",
                block: "statement_block",
                targets: &[
                    "function_declaration",
                    "generator_function_declaration",
                    "method_definition",
                ],
                bindings: &["variable_declarator", "public_field_definition"],
            }),
            _ => anyhow::bail!(
                "body replacement supports Rust, TypeScript and TSX; select a supported function."
            ),
        }
    }

    fn block_span(&self, body: tree_sitter::Node<'_>) -> Result<Span> {
        ensure!(
            body.kind() == self.block,
            "selected function needs a block body."
        );
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

fn replacement(path: &Path, language: Language, syntax: &BodySyntax) -> Result<String> {
    let text = fragment(path)?;
    let prefix = syntax.prefix;
    let wrapped = format!("{prefix}{text}");
    let parsed = Parsers::new().parse(language, &wrapped)?;
    let item = parsed
        .root()
        .named_child(0)
        .context("replacement needs a block in the selected language.")?;
    let body = item
        .child_by_field_name("body")
        .context("replacement needs a block in the selected language.")?;
    let span = syntax.block_span(body)?;
    ensure!(
        !parsed.has_errors()
            && parsed.root().named_child_count() == 1
            && item.kind() == syntax.item
            && span.start == prefix.len()
            && span.end == wrapped.len(),
        "replacement must contain exactly one complete block in the selected language."
    );
    Ok(text)
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
    ensure!(
        !parsed.has_errors() && items.next().is_none() && start == 0
            && item.kind() == "function_item" && item.end_byte() == text.len()
            && item.child_by_field_name("body").is_some(),
        "fragment requires one Rust function with a body; only insertion accepts leading outer documentation comments."
    );
    Ok(item)
}

impl Project<'_> {
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
        let (container, offset) = if is_file {
            (parsed.root(), source.len())
        } else {
            let symbol = self.nodes[id]
                .symbol
                .and_then(|id| self.index.symbol(id))
                .context("declaration insertion requires a Rust file or inline module handle.")?;
            let mut selected = parsed
                .root()
                .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end);
            let module = loop {
                let node = selected.context("select a Rust file or inline module handle.")?;
                if node.kind() == "mod_item" {
                    ensure!(
                        node.child_by_field_name("name")
                            .is_some_and(|name| Span::from(name) == symbol.name_span),
                        "selected handle does not name this module."
                    );
                    break node;
                }
                selected = node.parent();
            };
            let body = module.child_by_field_name("body")
                .context("external module declarations have no inline body; select their source file instead.")?;
            ensure!(
                body.kind() == "declaration_list",
                "module requires an inline declaration list."
            );
            let mut cursor = body.walk();
            let close = body
                .children(&mut cursor)
                .find(|child| child.kind() == "}" && !child.is_missing())
                .context("inline module needs a closing brace.")?;
            let before = &source[..close.start_byte()];
            let line_start = before.rfind('\n').map_or(0, |at| at + 1);
            let offset = if line_start > body.start_byte()
                && before[line_start..]
                    .bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
            {
                line_start
            } else {
                close.start_byte()
            };
            (body, offset)
        };
        let text = fragment(&self.root.join(&options.from))?;
        let fragment_tree = Parsers::new().parse(Language::Rust, &text)?;
        let function = function_fragment(&fragment_tree, &text, true)?;
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
        let body = function
            .child_by_field_name("body")
            .context("function needs a body.")?;
        let mut report = self.envelope("insert-declaration");
        report["schema"] = json!("fr-author-1");
        report["handle"] = json!(self.handle(id));
        report["path"] = bounded_text(&self.nodes[id].path.to_string_lossy(), 512);
        report["signature"] = json!({"basis": "syntax-header", "text": bounded_text(text[function.start_byte()..body.start_byte()].trim_end(), 512)});
        report["declaration"] = json!({"name": bounded_text(name, 512), "kind": "function", "span": Span::new(offset+leading.len(), offset+leading.len()+text.len()), "bytes": text.len(), "sha256": digest(&text)});
        if function.start_byte() > 0 {
            let documentation = &text[..function.start_byte()];
            let start = offset + leading.len();
            report["documentation"] = json!({"kind": "outer-doc-comments", "span": Span::new(start, start + documentation.len()), "bytes": documentation.len(), "sha256": digest(documentation)});
        }
        report["insertion"] = json!({"before_span": span, "after_span": Span::new(span.start, span.start+inserted.len()), "added_bytes": inserted.len(), "leading_separator": leading, "trailing_separator": newline, "sha256": digest(&inserted)});
        report["name_check"] = json!(if is_file {
            "direct top-level item names; Rust namespaces are not distinguished"
        } else {
            "direct items in the selected module. Rust namespaces are not distinguished."
        });
        if !is_file {
            report["container"] = json!({"kind": "inline-module", "name": bounded_text(&self.nodes[id].name, 512),
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
        let item = function_fragment(&replacement, &after, false)?;
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
            let node = selected.context(
                "select a function declaration, method or function binding with a block body.",
            )?;
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
        let span = syntax.block_span(body)?;
        let before = &source[span.start..span.end];
        let after = replacement(&self.root.join(&options.from), language, &syntax)?;
        ensure!(
            super::body_replacement_budget(before.len(), after.len()),
            "old and new bodies must each fit 2 through 65536 bytes."
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
        report["changed"] = json!(before != after);
        report["validation"] = json!("reparse-strict");
        report["preservation"] = json!("bytes outside the selected body");
        report["behavior_checked"] = json!(false);
        report["atomic_snapshot"] = json!(false);
        Ok(Plan { edits, report })
    }
}
