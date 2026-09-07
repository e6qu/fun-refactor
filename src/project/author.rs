use super::{bounded_text, Project};
use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::parse::Parsers;
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
    #[command(about = "Replace one Rust function body while retaining its surrounding source.")]
    ReplaceBody(ReplaceBodyOptions),
}

#[derive(Args)]
pub struct ReplaceBodyOptions {
    #[arg(help = "Full project handle, or a short ID with --revision.")]
    pub handle: String,
    #[arg(long, help = "Source revision required for a short ID.")]
    pub revision: Option<String>,
    #[arg(
        long,
        help = "UTF-8 block file, absolute or relative to the workspace root; at most 64 KiB."
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
        help = "Record and apply the replacement after checking the source revision."
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

fn replacement(path: &Path) -> Result<String> {
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
    let text = text.trim();
    let prefix = "fn __fr_body__() ";
    let wrapped = format!("{prefix}{text}");
    let parsed = Parsers::new().parse(Language::Rust, &wrapped)?;
    let item = parsed
        .root()
        .named_child(0)
        .context("replacement needs a Rust block.")?;
    let body = item
        .child_by_field_name("body")
        .context("replacement needs a Rust block.")?;
    ensure!(
        !parsed.has_errors()
            && parsed.root().named_child_count() == 1
            && item.kind() == "function_item"
            && body.kind() == "block"
            && body.start_byte() == prefix.len()
            && body.end_byte() == wrapped.len(),
        "replacement must contain exactly one complete Rust block."
    );
    Ok(text.to_owned())
}

impl Project<'_> {
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
        ensure!(
            symbol.language == Language::Rust,
            "body replacement currently supports Rust functions only."
        );
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(Language::Rust, source)?;
        ensure!(
            !parsed.has_errors(),
            "body replacement requires a file without parser errors."
        );
        let mut selected = parsed
            .root()
            .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end);
        let function = loop {
            let node =
                selected.context("selected declaration is not a Rust function with a body.")?;
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
        let body = function
            .child_by_field_name("body")
            .context("selected function has no body.")?;
        ensure!(
            body.kind() == "block",
            "selected function does not have a Rust block body."
        );
        let span = Span::from(body);
        let before = &source[span.start..span.end];
        let after = replacement(&self.root.join(&options.from))?;
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
            !Parsers::new().parse(Language::Rust, &updated)?.has_errors(),
            "replacement introduces parser errors in its destination context."
        );
        let mut report = self.envelope("replace-body");
        report["schema"] = json!("fr-author-1");
        report["handle"] = json!(self.handle(id));
        report["path"] = bounded_text(&self.nodes[id].path.to_string_lossy(), 512);
        report["signature"] = self.signature(id)?;
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
