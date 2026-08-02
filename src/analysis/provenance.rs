//! Config-language value provenance: where a configured value comes from, and
//! what consumes it.
//!
//! # What this is, and why it is not [`super::flow`]
//!
//! Imperative languages need dataflow: values move through assignments, calls and
//! branches, and the analysis is an approximation of an execution. Configuration
//! languages do not execute — they *evaluate*, by substitution and override, under
//! a model each language specifies exactly:
//!
//! - **Terraform**: `var.x`, `local.y` and `module.m.out` form a true substitution
//!   DAG. Checkov builds the same graph but substitutes values in place, which
//!   destroys the hop chain; here every hop is retained with its own file, line and
//!   expression text, and nothing is ever rewritten.
//! - **Helm**: a values key has a defined override order — subchart defaults, then
//!   each enclosing parent chart, then user-supplied `-f` files, then `--set`. Every
//!   competing source visible in the workspace is reported with its precedence, the
//!   winner is marked, and the losers stay visible.
//! - **CSS**: the cascade *is* a spec'd provenance algorithm (origin → layer →
//!   specificity → source order). Losing declarations are reported, struck through
//!   rather than discarded, which is the DevTools model.
//! - **YAML**: an alias takes its value from its anchor. Anchors are discarded after
//!   composition, so this has to be read off the CST — which is what the index does.
//!
//! # Where it stops
//!
//! Everything this cannot determine is a [`StopReason`], never a guess:
//!
//! - a Terraform input variable's value comes from `*.tfvars`, `-var` or `TF_VAR_*`
//!   — outside the code entirely ([`StopReason::ExternalInput`]);
//! - a Helm value read inside a `{{ ... }}` action is masked before parsing
//!   (`src/parse.rs`) and its use is decided by the template engine at render time
//!   ([`StopReason::RenderDependent`]);
//! - a resource attribute is computed by a provider at apply time
//!   ([`StopReason::ComputedAtApply`]);
//! - competing sources whose relative order is not visible in the workspace — two
//!   `-f` files, two `@layer`s, two stylesheets — are all listed and the winner is
//!   left undecided ([`StopReason::PrecedenceUndetermined`]).

use crate::index::Index;
use crate::lang::{Language, LanguageClass};
use crate::model::{Confidence, Reference, ReferenceKind, Symbol, SymbolId, SymbolKind};
use crate::parse::Parsers;
use crate::span::{LineIndex, Span};
use anyhow::{anyhow, bail, Result};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Which way the provenance walk runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Where does this value come from?
    Backward,
    /// What consumes this value?
    Forward,
}

/// What kind of provenance edge one hop crossed.
///
/// These are the `PROVENANCE` edge labels: substitution, override, expansion and
/// default, plus the language-specific forms each of those takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// The declaration the query started from.
    Declaration,
    /// One value spliced into another: `var.x`, `local.y`, `${...}`.
    Substitution,
    /// A source that competes with others under a precedence order.
    Override,
    /// A declared default, used only when nothing overrides it.
    Default,
    /// A YAML alias expanding its anchor.
    Expansion,
    /// An output read from another Terraform module.
    ModuleOutput,
    /// A `{{ ... }}` template action: the link is textual, the value is render-time.
    TemplateAction,
    /// A CSS `var()` reference.
    VarFunction,
    /// A use site (forward direction).
    Use,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Declaration => "declaration",
            EdgeKind::Substitution => "substitution",
            EdgeKind::Override => "override",
            EdgeKind::Default => "default",
            EdgeKind::Expansion => "expansion",
            EdgeKind::ModuleOutput => "module-output",
            EdgeKind::TemplateAction => "template-action",
            EdgeKind::VarFunction => "var()",
            EdgeKind::Use => "use",
        }
    }
}

/// Why a provenance chain stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// A literal, or anything else that is its own source.
    Origin(String),
    /// The value is set outside the code: tfvars, `-var`, `TF_VAR_*`, `--set`, `-f`.
    ///
    /// `required` distinguishes "nothing in the workspace supplies this at all"
    /// from "the workspace supplies a value that an external source may override".
    ExternalInput {
        name: String,
        required: bool,
        sources: String,
    },
    /// A reference that names nothing findable in the workspace.
    Unresolved(String),
    /// The depth limit was reached; more may lie beyond.
    DepthLimit,
    /// The value is decided inside a `{{ ... }}` template action, which is masked
    /// before parsing and evaluated by the template engine, not by us.
    RenderDependent(String),
    /// A provider computes this at apply time; no configuration holds it.
    ComputedAtApply(String),
    /// Several sources compete and the workspace does not show which wins.
    PrecedenceUndetermined(String),
    /// A config language with no value-substitution model to follow.
    UnsupportedLanguage(Language),
    /// The symbol holds no value at all (a resource block, a chart heading…).
    NotAValue(String),
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::Origin(what) => write!(f, "origin: {what}"),
            StopReason::ExternalInput {
                name,
                required,
                sources,
            } => {
                if *required {
                    write!(
                        f,
                        "'{name}' is an external input with no value in the workspace; it comes from {sources}"
                    )
                } else {
                    write!(f, "'{name}' can still be overridden externally, from {sources}")
                }
            }
            StopReason::Unresolved(what) => write!(f, "'{what}' resolves to nothing in the workspace"),
            StopReason::DepthLimit => write!(f, "depth limit reached; more may lie beyond"),
            StopReason::RenderDependent(what) => write!(
                f,
                "render-dependent: {what} is decided inside a template action, which is masked before parsing and evaluated at render time"
            ),
            StopReason::ComputedAtApply(what) => write!(
                f,
                "'{what}' is computed by its provider at apply time; no configuration holds this value"
            ),
            StopReason::PrecedenceUndetermined(what) => {
                write!(f, "precedence undetermined: {what}")
            }
            StopReason::UnsupportedLanguage(lang) => write!(
                f,
                "{lang} has no value-substitution model; provenance covers terraform/hcl, yaml/helm and css"
            ),
            StopReason::NotAValue(what) => write!(f, "{what} declares no value to trace"),
        }
    }
}

/// One hop in a provenance chain. Hops are never collapsed or substituted away:
/// each keeps its own file, line and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub symbol: Option<SymbolId>,
    pub kind: EdgeKind,
    /// The expression exactly as written at this hop.
    pub text: String,
    pub file: PathBuf,
    pub span: Span,
    /// 1-based line of `span` in `file`.
    pub line: usize,
    pub depth: usize,
    pub confidence: Confidence,
}

/// CSS specificity as the spec's (a, b, c) triple: ids, class-likes, elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity {
    pub ids: u32,
    pub classes: u32,
    pub elements: u32,
}

impl std::fmt::Display for Specificity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({},{},{})", self.ids, self.classes, self.elements)
    }
}

/// Where a source sits in its language's override order. Higher `rank` wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Precedence {
    pub rank: u32,
    /// Human-readable level, e.g. "subchart defaults" or "author stylesheet".
    pub label: String,
    /// Set for CSS, where the cascade compares specificity before source order.
    pub specificity: Option<Specificity>,
    /// Set for CSS `!important`, which reverses origin order.
    pub important: bool,
    /// The `@layer` a CSS declaration belongs to, if any.
    pub layer: Option<String>,
}

impl Precedence {
    fn level(rank: u32, label: impl Into<String>) -> Self {
        Self {
            rank,
            label: label.into(),
            specificity: None,
            important: false,
            layer: None,
        }
    }
}

/// One of several sources competing to supply the same value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetingSource {
    pub hop: Hop,
    pub precedence: Precedence,
    /// True for the source that supplies the value, as far as the workspace shows.
    pub wins: bool,
    /// Why this source won or lost.
    pub reason: String,
}

/// Every source competing to supply one value, with the winner marked.
///
/// Losers are retained deliberately: an override is only understandable next to
/// what it overrode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Competition {
    /// What is being competed for, e.g. `values key image.tag`.
    pub subject: String,
    /// The precedence model applied, stated in full.
    pub model: String,
    /// False when something outside the workspace could still change the answer.
    pub decided: bool,
    /// Sorted strongest first.
    pub sources: Vec<CompetingSource>,
}

impl Competition {
    pub fn winner(&self) -> Option<&CompetingSource> {
        self.sources.iter().find(|s| s.wins)
    }

    pub fn losers(&self) -> Vec<&CompetingSource> {
        self.sources.iter().filter(|s| !s.wins).collect()
    }
}

/// The result of a provenance query.
#[derive(Debug, Clone)]
pub struct Provenance {
    pub direction: Direction,
    pub root: SymbolId,
    /// The hop chain, in visit order. Immutable history: nothing is collapsed.
    pub hops: Vec<Hop>,
    pub competitions: Vec<Competition>,
    /// Every boundary the walk refused to cross, so gaps stay visible.
    pub stops: Vec<(usize, StopReason)>,
}

impl Provenance {
    pub fn is_empty(&self) -> bool {
        self.hops.is_empty()
    }

    /// The weakest link in the chain — the honest confidence of the whole answer.
    pub fn weakest_confidence(&self) -> Option<Confidence> {
        self.hops.iter().map(|h| h.confidence).max()
    }

    /// Does any stop match `predicate`?
    pub fn stopped_because(&self, predicate: impl Fn(&StopReason) -> bool) -> bool {
        self.stops.iter().any(|(_, r)| predicate(r))
    }

    pub fn format_tree(&self) -> String {
        let mut out = String::new();
        for hop in &self.hops {
            let confidence = if hop.confidence.is_safe_to_rewrite() {
                String::new()
            } else {
                format!(" [{}]", hop.confidence.as_str())
            };
            out.push_str(&format!(
                "{}{} {}  ({}:{}){}\n",
                "  ".repeat(hop.depth),
                hop.kind.as_str(),
                hop.text,
                hop.file.display(),
                hop.line,
                confidence
            ));
        }
        for competition in &self.competitions {
            out.push_str(&format!(
                "\n{} — {}{}\n",
                competition.subject,
                competition.model,
                if competition.decided {
                    ""
                } else {
                    " (not decidable from the workspace alone)"
                }
            ));
            for source in &competition.sources {
                out.push_str(&format!(
                    "  {} {} [{}]  ({}:{}) — {}\n",
                    if source.wins { "WINS " } else { "loses" },
                    source.hop.text,
                    source.precedence.label,
                    source.hop.file.display(),
                    source.hop.line,
                    source.reason
                ));
            }
        }
        if !self.stops.is_empty() {
            out.push_str("\nStopped at:\n");
            for (depth, reason) in &self.stops {
                out.push_str(&format!("{}- {reason}\n", "  ".repeat(*depth)));
            }
        }
        out
    }
}

/// Does provenance analysis apply to this file's language?
///
/// The mirror of [`super::flow::applies_to`]: imperative languages get dataflow,
/// config languages get substitution/override provenance.
pub fn applies_to(index: &Index, file: &Path) -> bool {
    index
        .file(file)
        .is_some_and(|info| info.language.class() == LanguageClass::Config)
}

/// Trace backwards: where does this configured value come from?
pub fn provenance(index: &Index, symbol: SymbolId, max_depth: usize) -> Result<Provenance> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow!("no symbol with id {symbol:?} in this index"))?;
    refuse_imperative(sym)?;

    let mut ctx = Ctx::new(index, max_depth, Direction::Backward, symbol);
    match sym.language {
        Language::Hcl => ctx.hcl_backward(sym, EdgeKind::Declaration, 0)?,
        Language::Yaml | Language::Helm => ctx.yaml_backward(sym, EdgeKind::Declaration, 0)?,
        Language::Css | Language::Scss => ctx.css_backward(sym, 0)?,
        other => ctx.stop(0, StopReason::UnsupportedLanguage(other)),
    }
    Ok(ctx.out)
}

/// Trace forwards: what consumes this configured value?
pub fn consumers(index: &Index, symbol: SymbolId, max_depth: usize) -> Result<Provenance> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow!("no symbol with id {symbol:?} in this index"))?;
    refuse_imperative(sym)?;

    let mut ctx = Ctx::new(index, max_depth, Direction::Forward, symbol);
    match sym.language {
        Language::Hcl => ctx.hcl_forward(sym, EdgeKind::Declaration, 0)?,
        Language::Yaml | Language::Helm => ctx.yaml_forward(sym, 0)?,
        Language::Css | Language::Scss => ctx.css_forward(sym, 0)?,
        other => ctx.stop(0, StopReason::UnsupportedLanguage(other)),
    }
    Ok(ctx.out)
}

/// Imperative languages are refused outright, pointing at the analysis that does
/// apply to them.
fn refuse_imperative(sym: &Symbol) -> Result<()> {
    if sym.language.class() == LanguageClass::Imperative {
        bail!(
            "{} is imperative: '{}' has a dataflow, not a substitution/override provenance. \
             Use analysis::flow (backward/forward) instead.",
            sym.language,
            sym.name
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- the walker

struct Ctx<'a> {
    index: &'a Index,
    max_depth: usize,
    out: Provenance,
    /// Symbols already expanded, so cyclic configuration terminates.
    seen: HashSet<SymbolId>,
    sources: HashMap<PathBuf, String>,
}

impl<'a> Ctx<'a> {
    fn new(index: &'a Index, max_depth: usize, direction: Direction, root: SymbolId) -> Self {
        Self {
            index,
            max_depth,
            out: Provenance {
                direction,
                root,
                hops: Vec::new(),
                competitions: Vec::new(),
                stops: Vec::new(),
            },
            seen: HashSet::new(),
            sources: HashMap::new(),
        }
    }

    fn stop(&mut self, depth: usize, reason: StopReason) {
        if !self.out.stops.iter().any(|(d, r)| *d == depth && r == &reason) {
            self.out.stops.push((depth, reason));
        }
    }

    fn source(&mut self, file: &Path) -> Result<String> {
        if let Some(text) = self.sources.get(file) {
            return Ok(text.clone());
        }
        let text = std::fs::read_to_string(file)?;
        self.sources.insert(file.to_path_buf(), text.clone());
        Ok(text)
    }

    /// Build a hop, resolving the line number from the file's text.
    fn hop(
        &mut self,
        symbol: Option<SymbolId>,
        kind: EdgeKind,
        text: impl Into<String>,
        file: &Path,
        span: Span,
        depth: usize,
    ) -> Result<Hop> {
        let source = self.source(file)?;
        let line = LineIndex::new(&source).line_col(span.start, &source).line;
        Ok(Hop {
            symbol,
            kind,
            text: text.into(),
            file: file.to_path_buf(),
            span,
            line,
            depth,
            // Callers that resolved a weaker edge override this with a struct update.
            confidence: Confidence::Exact,
        })
    }

    fn push_hop(&mut self, hop: Hop) {
        self.out.hops.push(hop);
    }

    /// Depth guard shared by every walk.
    fn over_depth(&mut self, depth: usize) -> bool {
        if depth > self.max_depth {
            self.stop(depth, StopReason::DepthLimit);
            return true;
        }
        false
    }

    /// References inside a byte range of one file, in source order.
    fn refs_in(&self, file: &Path, span: Span) -> Vec<&'a Reference> {
        let Some(info) = self.index.file(file) else {
            return Vec::new();
        };
        let mut found: Vec<&Reference> = info
            .references
            .iter()
            .map(|i| &self.index.references[*i])
            .filter(|r| span.contains(r.span))
            .collect();
        found.sort_by_key(|r| r.span.start);
        found
    }
}

// ------------------------------------------------------------------ Terraform

/// What a Terraform symbol is, which decides where its value lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HclRole {
    /// `variable "x" {}` — an input, set from outside the module.
    InputVariable,
    /// An attribute of a `locals` block.
    Local,
    /// `output "x" {}`.
    Output,
    /// `module "x" {}`.
    Module,
    /// `resource`/`data` and every other block: not a value.
    Block,
}

/// Terraform's reserved evaluation-context namespaces: values the engine supplies.
const HCL_CONTEXT_NAMESPACES: &[&str] = &["each", "count", "self", "path", "terraform"];

fn hcl_role(sym: &Symbol, source: &str) -> HclRole {
    let head = sym.full_span.text(source);
    match sym.kind {
        SymbolKind::Module => HclRole::Module,
        SymbolKind::Variable if head.starts_with("variable") => HclRole::InputVariable,
        SymbolKind::Variable => HclRole::Local,
        _ if head.starts_with("output") => HclRole::Output,
        _ => HclRole::Block,
    }
}

/// The Terraform address a symbol is referenced by.
fn hcl_address(sym: &Symbol, role: HclRole) -> String {
    match role {
        HclRole::InputVariable => format!("var.{}", sym.name),
        HclRole::Local => format!("local.{}", sym.name),
        HclRole::Output => format!("output.{}", sym.name),
        HclRole::Module => format!("module.{}", sym.name),
        HclRole::Block => sym
            .qualifier
            .as_ref()
            .map(|q| format!("{q}.{}", sym.name))
            .unwrap_or_else(|| sym.name.clone()),
    }
}

impl Ctx<'_> {
    fn hcl_backward(&mut self, sym: &Symbol, edge: EdgeKind, depth: usize) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        if !self.seen.insert(sym.id) {
            return Ok(());
        }
        let source = self.source(&sym.file)?;
        let role = hcl_role(sym, &source);
        let address = hcl_address(sym, role);
        let value = hcl_value_span(&source, sym, role)?;

        let text = match value {
            Some(span) => format!("{address} = {}", snippet(span.text(&source))),
            None => format!("{address}: {}", snippet(sym.full_span.text(&source))),
        };
        let hop = self.hop(
            Some(sym.id),
            edge,
            text,
            &sym.file,
            sym.full_span,
            depth,
        )?;
        self.push_hop(hop);

        match role {
            HclRole::InputVariable => {
                self.hcl_variable_sources(sym, value, depth)?;
                Ok(())
            }
            HclRole::Local | HclRole::Output => match value {
                Some(span) => self.hcl_follow(&sym.file, &source, span, depth),
                None => {
                    self.stop(
                        depth,
                        StopReason::NotAValue(format!("{address} (no value expression)")),
                    );
                    Ok(())
                }
            },
            HclRole::Module => {
                self.stop(
                    depth,
                    StopReason::NotAValue(format!(
                        "{address} is a module call, not a value; trace one of its outputs"
                    )),
                );
                Ok(())
            }
            HclRole::Block => {
                self.stop(
                    depth,
                    StopReason::NotAValue(format!("{address} is a block, not a value")),
                );
                Ok(())
            }
        }
    }

    /// An input variable's value comes from outside the module. Every source the
    /// workspace can see is reported, in Terraform's documented precedence order.
    fn hcl_variable_sources(
        &mut self,
        sym: &Symbol,
        default: Option<Span>,
        depth: usize,
    ) -> Result<()> {
        let source = self.source(&sym.file)?;
        let mut sources: Vec<(u32, String, PathBuf, Span, String)> = Vec::new();

        if let Some(span) = default {
            sources.push((
                0,
                "default".to_string(),
                sym.file.clone(),
                span,
                format!("default = {}", snippet(span.text(&source))),
            ));
        }

        // Terraform's order, lowest first: default < TF_VAR_* < terraform.tfvars <
        // *.auto.tfvars (alphabetical) < -var/-var-file. Only the files are visible.
        let dir = sym.file.parent().map(Path::to_path_buf);
        let tfvars: Vec<PathBuf> = self
            .index
            .files()
            .map(|(p, _)| p.clone())
            .filter(|p| p.parent().map(Path::to_path_buf) == dir)
            .filter(|p| file_name(p).ends_with(".tfvars"))
            .collect();
        for path in tfvars {
            let name = file_name(&path);
            let rank = if name == "terraform.tfvars" { 2 } else { 3 };
            let label = if rank == 2 {
                "terraform.tfvars".to_string()
            } else {
                format!("{name} (auto-loaded)")
            };
            let text = self.source(&path)?;
            if let Some((span, value)) = tfvars_entry(&text, &sym.name)? {
                sources.push((
                    rank,
                    label,
                    path.clone(),
                    span,
                    format!("{} = {}", sym.name, snippet(&value)),
                ));
            }
        }

        let required = sources.is_empty();
        if !sources.is_empty() {
            // Ties within a rank are broken by file name, matching the alphabetical
            // load order of `*.auto.tfvars`.
            sources.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| file_name(&a.2).cmp(&file_name(&b.2))));
            let winner = sources.len() - 1;
            let mut competing = Vec::new();
            for (i, (rank, label, path, span, text)) in sources.iter().enumerate() {
                // The `default` attribute is a fallback the language declares; the
                // tfvars entries are overrides of it.
                let edge = if *rank == 0 {
                    EdgeKind::Default
                } else {
                    EdgeKind::Override
                };
                let hop = self.hop(None, edge, text.clone(), path, *span, depth + 1)?;
                competing.push(CompetingSource {
                    hop,
                    precedence: Precedence::level(*rank, label.clone()),
                    wins: i == winner,
                    reason: if i == winner {
                        "highest-precedence source visible in the workspace".to_string()
                    } else {
                        format!(
                            "overridden by '{}'",
                            sources[winner].1
                        )
                    },
                });
            }
            competing.reverse();
            for source in &competing {
                self.push_hop(source.hop.clone());
            }
            self.out.competitions.push(Competition {
                subject: format!("input variable var.{}", sym.name),
                model: "terraform: default < TF_VAR_* < terraform.tfvars < *.auto.tfvars < -var/-var-file"
                    .to_string(),
                decided: false,
                sources: competing,
            });
        }

        self.stop(
            depth,
            StopReason::ExternalInput {
                name: format!("var.{}", sym.name),
                required,
                sources: "a *.tfvars file, -var/-var-file on the CLI, or TF_VAR_ in the environment"
                    .to_string(),
            },
        );
        Ok(())
    }

    /// Follow every reference inside a Terraform expression to its declaration.
    fn hcl_follow(&mut self, file: &Path, source: &str, value: Span, depth: usize) -> Result<()> {
        let references = self.refs_in(file, value);
        if references.is_empty() {
            self.stop(
                depth,
                StopReason::Origin(format!("literal value {}", snippet(value.text(source)))),
            );
            return Ok(());
        }

        let mut followed = 0usize;
        let mut i = 0;
        while i < references.len() {
            let reference = references[i];
            let namespace = namespace_before(source, reference.span);
            match (reference.kind, namespace.as_deref()) {
                (ReferenceKind::Identifier, Some("var")) => {
                    followed += 1;
                    self.hcl_resolve_and_walk(file, reference, HclRole::InputVariable, depth)?;
                }
                (ReferenceKind::Identifier, Some("local")) => {
                    followed += 1;
                    self.hcl_resolve_and_walk(file, reference, HclRole::Local, depth)?;
                }
                (ReferenceKind::Identifier, Some("module")) => {
                    followed += 1;
                    // The output name is the next segment of the same traversal.
                    let output = references
                        .get(i + 1)
                        .filter(|r| r.kind == ReferenceKind::Field)
                        .map(|r| r.name.clone());
                    self.hcl_module_output(file, reference, output.as_deref(), depth)?;
                    if output.is_some() {
                        i += 1;
                    }
                }
                (ReferenceKind::Identifier, Some(kind)) => {
                    followed += 1;
                    // `aws_s3_bucket.main.arn` — a managed resource attribute.
                    let attribute = references
                        .get(i + 1)
                        .filter(|r| r.kind == ReferenceKind::Field)
                        .map(|r| format!(".{}", r.name))
                        .unwrap_or_default();
                    self.stop(
                        depth + 1,
                        StopReason::ComputedAtApply(format!("{kind}.{}{attribute}", reference.name)),
                    );
                }
                (ReferenceKind::Field, Some(namespace))
                    if HCL_CONTEXT_NAMESPACES.contains(&namespace) =>
                {
                    followed += 1;
                    self.stop(
                        depth + 1,
                        StopReason::Origin(format!(
                            "{namespace}.{} is supplied by Terraform's evaluation context",
                            reference.name
                        )),
                    );
                }
                // A trailing attribute or type label already accounted for by the
                // address it belongs to, and function names, which transform the
                // arguments that are followed separately.
                _ => {}
            }
            i += 1;
        }

        if followed == 0 {
            self.stop(
                depth,
                StopReason::Origin(format!("literal value {}", snippet(value.text(source)))),
            );
        }
        Ok(())
    }

    /// Resolve `var.x` / `local.x` within the module (= the directory) and recurse.
    fn hcl_resolve_and_walk(
        &mut self,
        file: &Path,
        reference: &Reference,
        role: HclRole,
        depth: usize,
    ) -> Result<()> {
        let candidates = self.hcl_candidates(file, &reference.name, role)?;
        match candidates.len() {
            0 => {
                let namespace = if role == HclRole::InputVariable {
                    "var"
                } else {
                    "local"
                };
                self.stop(
                    depth + 1,
                    StopReason::Unresolved(format!("{namespace}.{}", reference.name)),
                );
            }
            1 => {
                let target = self.index.symbol(candidates[0]).expect("candidate exists");
                self.hcl_backward(target, EdgeKind::Substitution, depth + 1)?;
            }
            _ => {
                // Terraform forbids duplicate declarations, so this means the
                // workspace holds more than one module; do not pick one.
                self.stop(
                    depth + 1,
                    StopReason::PrecedenceUndetermined(format!(
                        "'{}' is declared {} times in this module directory",
                        reference.name,
                        candidates.len()
                    )),
                );
            }
        }
        Ok(())
    }

    /// Symbols in the same module directory playing `role`.
    fn hcl_candidates(&mut self, file: &Path, name: &str, role: HclRole) -> Result<Vec<SymbolId>> {
        let dir = file.parent().map(Path::to_path_buf);
        let same_dir: Vec<&Symbol> = self
            .index
            .symbols
            .iter()
            .filter(|s| s.language == Language::Hcl && s.name == name)
            .filter(|s| s.file.parent().map(Path::to_path_buf) == dir)
            .collect();
        let mut out = Vec::new();
        for symbol in same_dir {
            let source = self.source(&symbol.file)?;
            if hcl_role(symbol, &source) == role {
                out.push(symbol.id);
            }
        }
        Ok(out)
    }

    /// `module.m.out` — resolve the module's `source` to a directory and find the
    /// `output "out"` it declares.
    fn hcl_module_output(
        &mut self,
        file: &Path,
        reference: &Reference,
        output: Option<&str>,
        depth: usize,
    ) -> Result<()> {
        let Some(output) = output else {
            self.stop(
                depth + 1,
                StopReason::Unresolved(format!(
                    "module.{} is read without naming an output",
                    reference.name
                )),
            );
            return Ok(());
        };
        let address = format!("module.{}.{output}", reference.name);

        let Some(info) = self.index.file(file) else {
            self.stop(depth + 1, StopReason::Unresolved(address));
            return Ok(());
        };
        let Some(import) = info
            .imports
            .iter()
            .find(|i| i.alias.as_deref() == Some(reference.name.as_str()))
        else {
            self.stop(
                depth + 1,
                StopReason::Unresolved(format!("{address} (no module block with a literal source)")),
            );
            return Ok(());
        };

        let Some(dir) = file
            .parent()
            .map(|d| normalise(&d.join(&import.path)))
            .filter(|d| {
                self.index
                    .files()
                    .any(|(p, _)| p.parent() == Some(d.as_path()))
            })
        else {
            self.stop(
                depth + 1,
                StopReason::Unresolved(format!(
                    "{address}: module source '{}' is not in the workspace",
                    import.path
                )),
            );
            return Ok(());
        };

        let mut targets = Vec::new();
        for symbol in self
            .index
            .symbols
            .iter()
            .filter(|s| s.language == Language::Hcl && s.name == output)
            .filter(|s| s.file.parent() == Some(dir.as_path()))
        {
            let source = self.source(&symbol.file)?;
            if hcl_role(symbol, &source) == HclRole::Output {
                targets.push(symbol.id);
            }
        }
        match targets.first() {
            Some(id) => {
                let target = self.index.symbol(*id).expect("output exists");
                self.hcl_backward(target, EdgeKind::ModuleOutput, depth + 1)
            }
            None => {
                self.stop(
                    depth + 1,
                    StopReason::Unresolved(format!(
                        "{address}: no output \"{output}\" in {}",
                        dir.display()
                    )),
                );
                Ok(())
            }
        }
    }

    /// Forward: which Terraform expressions read this declaration?
    fn hcl_forward(&mut self, sym: &Symbol, edge: EdgeKind, depth: usize) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        if !self.seen.insert(sym.id) {
            return Ok(());
        }
        let source = self.source(&sym.file)?;
        let role = hcl_role(sym, &source);
        let address = hcl_address(sym, role);
        let hop = self.hop(
            Some(sym.id),
            edge,
            format!("{address}: {}", snippet(sym.full_span.text(&source))),
            &sym.file,
            sym.full_span,
            depth,
        )?;
        self.push_hop(hop);

        let namespace = match role {
            HclRole::InputVariable => "var",
            HclRole::Local => "local",
            HclRole::Output => {
                self.hcl_output_consumers(sym, depth)?;
                return Ok(());
            }
            _ => {
                self.stop(
                    depth,
                    StopReason::NotAValue(format!("{address} is not a substitutable value")),
                );
                return Ok(());
            }
        };

        let dir = sym.file.parent().map(Path::to_path_buf);
        let uses: Vec<(PathBuf, Span)> = self
            .index
            .references
            .iter()
            .filter(|r| r.language == Language::Hcl && r.name == sym.name)
            .filter(|r| r.file.parent().map(Path::to_path_buf) == dir)
            .map(|r| (r.file.clone(), r.span))
            .collect();

        let mut found = 0;
        for (file, span) in uses {
            let text = self.source(&file)?;
            if namespace_before(&text, span).as_deref() != Some(namespace) {
                continue;
            }
            found += 1;
            self.hcl_use_site(&file, span, depth + 1)?;
        }
        if found == 0 {
            self.stop(
                depth,
                StopReason::Origin(format!("{address} is read nowhere in this module")),
            );
        }
        Ok(())
    }

    /// Record a use site and, when it sits inside another declaration, keep going.
    fn hcl_use_site(&mut self, file: &Path, span: Span, depth: usize) -> Result<()> {
        let text = self.source(file)?;
        let line = LineIndex::new(&text).line_col(span.start, &text).line;
        let line_text = LineIndex::new(&text)
            .line_span(line)
            .map(|s| s.text(&text).trim().to_string())
            .unwrap_or_default();
        let hop = self.hop(
            None,
            EdgeKind::Use,
            line_text,
            file,
            span,
            depth,
        )?;
        self.push_hop(hop);

        // The innermost declaration containing the use is what the value flows into.
        let container = self
            .index
            .file(file)
            .map(|info| {
                info.symbols
                    .iter()
                    .filter_map(|id| self.index.symbol(*id))
                    .filter(|s| s.full_span.contains(span))
                    .min_by_key(|s| s.full_span.len())
                    .map(|s| s.id)
            })
            .unwrap_or_default();
        if let Some(id) = container {
            let symbol = self.index.symbol(id).expect("container exists");
            let source = self.source(&symbol.file)?;
            if matches!(hcl_role(symbol, &source), HclRole::Local | HclRole::Output) {
                self.hcl_forward(symbol, EdgeKind::Substitution, depth + 1)?;
            }
        }
        Ok(())
    }

    /// An output is consumed by the calling module as `module.<alias>.<output>`.
    fn hcl_output_consumers(&mut self, sym: &Symbol, depth: usize) -> Result<()> {
        let Some(dir) = sym.file.parent().map(Path::to_path_buf) else {
            return Ok(());
        };
        let callers: Vec<(PathBuf, String)> = self
            .index
            .files()
            .filter(|(_, info)| info.language == Language::Hcl)
            .flat_map(|(path, info)| {
                info.imports.iter().filter_map({
                    let path = path.clone();
                    let dir = dir.clone();
                    move |import| {
                        let resolved = path.parent().map(|d| normalise(&d.join(&import.path)))?;
                        (resolved == dir)
                            .then(|| (path.clone(), import.alias.clone().unwrap_or_default()))
                    }
                })
            })
            .collect();

        if callers.is_empty() {
            self.stop(
                depth,
                StopReason::ExternalInput {
                    name: format!("output.{}", sym.name),
                    required: false,
                    sources: "whoever calls this module; no caller is present in the workspace"
                        .to_string(),
                },
            );
            return Ok(());
        }

        for (caller, alias) in callers {
            let text = self.source(&caller)?;
            let uses: Vec<Span> = self
                .refs_in(&caller, Span::new(0, text.len()))
                .into_iter()
                .filter(|r| r.name == sym.name && r.kind == ReferenceKind::Field)
                .filter(|r| {
                    // `module.<alias>.<name>` — the segment before ours is the alias.
                    namespace_before(&text, r.span).as_deref() == Some(alias.as_str())
                })
                .map(|r| r.span)
                .collect();
            if uses.is_empty() {
                self.stop(
                    depth + 1,
                    StopReason::Origin(format!(
                        "module \"{alias}\" in {} reads no output named {}",
                        caller.display(),
                        sym.name
                    )),
                );
            }
            for span in uses {
                self.hcl_use_site(&caller, span, depth + 1)?;
            }
        }
        Ok(())
    }
}

/// The expression a Terraform declaration binds, if it binds one.
fn hcl_value_span(source: &str, sym: &Symbol, role: HclRole) -> Result<Option<Span>> {
    let parsed = Parsers::new().parse(Language::Hcl, source)?;
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
    else {
        return Ok(None);
    };
    let value = match role {
        HclRole::Local => child_of_kind(node, "expression"),
        HclRole::InputVariable => block_attribute(node, source, "default"),
        HclRole::Output => block_attribute(node, source, "value"),
        _ => None,
    };
    Ok(value.map(Span::from))
}

/// The `expression` of a named attribute inside a block body.
fn block_attribute<'t>(block: Node<'t>, source: &str, name: &str) -> Option<Node<'t>> {
    let body = child_of_kind(block, "body")?;
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() != "attribute" {
            continue;
        }
        let key = child_of_kind(child, "identifier")?;
        if Span::from(key).text(source) == name {
            return child_of_kind(child, "expression");
        }
    }
    None
}

/// A top-level `name = value` entry of a `.tfvars` file.
///
/// Values files declare no symbols (they are plain attributes, not addressable
/// declarations), so this reads the CST directly rather than the index.
fn tfvars_entry(source: &str, name: &str) -> Result<Option<(Span, String)>> {
    let parsed = Parsers::new().parse(Language::Hcl, source)?;
    let Some(body) = child_of_kind(parsed.root(), "body") else {
        return Ok(None);
    };
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() != "attribute" {
            continue;
        }
        let Some(key) = child_of_kind(child, "identifier") else {
            continue;
        };
        if Span::from(key).text(source) != name {
            continue;
        }
        let value = child_of_kind(child, "expression").unwrap_or(child);
        return Ok(Some((
            Span::from(child),
            Span::from(value).text(source).to_string(),
        )));
    }
    Ok(None)
}

// --------------------------------------------------------------- YAML / Helm

/// A candidate source for one Helm values key.
struct ValuesSource {
    rank: u32,
    label: String,
    file: PathBuf,
    symbol: SymbolId,
}

impl Ctx<'_> {
    fn yaml_backward(&mut self, sym: &Symbol, edge: EdgeKind, depth: usize) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        if !self.seen.insert(sym.id) {
            return Ok(());
        }
        let source = self.source(&sym.file)?;
        let hop = self.hop(
            Some(sym.id),
            edge,
            snippet(sym.full_span.text(&source)),
            &sym.file,
            sym.full_span,
            depth,
        )?;
        self.push_hop(hop);

        if sym.kind == SymbolKind::Anchor {
            // An anchor is the origin of every alias that names it.
            self.stop(
                depth,
                StopReason::Origin(format!(
                    "anchor &{} holds {}",
                    sym.name,
                    snippet(sym.full_span.text(&source))
                )),
            );
            return Ok(());
        }
        if sym.kind != SymbolKind::Key {
            self.stop(
                depth,
                StopReason::NotAValue(format!("{} '{}'", sym.kind.as_str(), sym.name)),
            );
            return Ok(());
        }

        // 1. A value written as an alias takes it from the anchor.
        let mut expanded = false;
        for reference in self.refs_in(&sym.file, sym.full_span) {
            let Some(target) = reference.target.and_then(|t| self.index.symbol(t)) else {
                self.stop(
                    depth + 1,
                    StopReason::Unresolved(format!("*{}", reference.name)),
                );
                continue;
            };
            if target.kind == SymbolKind::Anchor {
                expanded = true;
                let target_id = target.id;
                let anchor = self.index.symbol(target_id).expect("anchor exists");
                self.yaml_backward(anchor, EdgeKind::Expansion, depth + 1)?;
            }
        }

        // 2. A value produced by a template action is decided at render time.
        let rendered = self.helm_template_value(sym, depth)?;

        // 3. Values-file precedence, for keys that are part of a chart's values.
        let competed = self.helm_values_competition(sym, depth)?;

        if !expanded && !rendered && !competed {
            let value = key_value_text(&source, sym);
            self.stop(
                depth,
                StopReason::Origin(match value {
                    Some(text) => format!("literal value {}", snippet(&text)),
                    None => format!("key '{}' with no scalar value of its own", sym.name),
                }),
            );
        }
        Ok(())
    }

    /// If this key's value is a `{{ ... }}` action, report it as render-dependent
    /// and follow any `.Values.*` it names — textually, because the action's bytes
    /// are masked out before parsing.
    fn helm_template_value(&mut self, sym: &Symbol, depth: usize) -> Result<bool> {
        if sym.language != Language::Helm {
            return Ok(false);
        }
        let source = self.source(&sym.file)?;
        let parsed = Parsers::new().parse(Language::Helm, &source)?;
        let lines = LineIndex::new(&source);
        let line = lines.line_col(sym.name_span.start, &source).line;
        let Some(line_span) = lines.line_span(line) else {
            return Ok(false);
        };
        let actions: Vec<Span> = parsed
            .template_actions
            .iter()
            .copied()
            .filter(|a| a.start >= sym.name_span.end && a.start < line_span.end)
            .collect();
        if actions.is_empty() {
            return Ok(false);
        }

        for action in actions {
            let text = action.text(&source).to_string();
            // The link out of a masked action is textual, so it never claims more
            // than a name match.
            let hop = Hop {
                confidence: Confidence::NameOnly,
                ..self.hop(
                    None,
                    EdgeKind::TemplateAction,
                    text.clone(),
                    &sym.file,
                    action,
                    depth + 1,
                )?
            };
            self.push_hop(hop);
            self.stop(depth + 1, StopReason::RenderDependent(text.clone()));

            for path in values_paths_in(&text) {
                self.helm_values_key(&sym.file, &path, depth + 2)?;
            }
            for builtin in builtins_in(&text) {
                self.stop(
                    depth + 2,
                    StopReason::ExternalInput {
                        name: builtin,
                        required: true,
                        sources: "Helm's built-in objects, supplied at render time".to_string(),
                    },
                );
            }
        }
        Ok(true)
    }

    /// Resolve a dotted `.Values` path against the chart's values files.
    fn helm_values_key(&mut self, from: &Path, path: &[String], depth: usize) -> Result<()> {
        let Some(chart) = chart_root(from) else {
            self.stop(
                depth,
                StopReason::Unresolved(format!(
                    ".Values.{} (no Chart.yaml above {})",
                    path.join("."),
                    from.display()
                )),
            );
            return Ok(());
        };
        let values = chart.join("values.yaml");
        let Some(symbol) = self.find_key(&values, path) else {
            self.stop(
                depth,
                StopReason::ExternalInput {
                    name: format!(".Values.{}", path.join(".")),
                    required: true,
                    sources: format!(
                        "outside the chart: no such key in {}, so it must come from -f or --set",
                        values.display()
                    ),
                },
            );
            return Ok(());
        };
        let target = self.index.symbol(symbol).expect("key exists");
        self.yaml_backward(target, EdgeKind::Substitution, depth)
    }

    /// Report every values file that supplies this key, in Helm's override order.
    fn helm_values_competition(&mut self, sym: &Symbol, depth: usize) -> Result<bool> {
        if !is_values_file(&sym.file) {
            return Ok(false);
        }
        let Some(chart) = chart_root(&sym.file) else {
            return Ok(false);
        };
        let path = self.key_path(sym);

        // Normalise to the chart that actually owns the key: a `mysql.image.tag`
        // entry in a parent chart addresses the `image.tag` of subchart `mysql`.
        let (owner, local) = self.descend_to_subchart(&chart, &path);
        let levels = chart_levels(&owner, &local);

        let mut candidates: Vec<ValuesSource> = Vec::new();
        for (rank, (dir, level_path)) in levels.iter().enumerate() {
            let files: Vec<PathBuf> = self
                .index
                .files()
                .map(|(p, _)| p.clone())
                .filter(|p| p.parent() == Some(dir.as_path()) && is_values_file(p))
                .collect();
            for file in files {
                let Some(symbol) = self.find_key(&file, level_path) else {
                    continue;
                };
                let is_defaults = file_name(&file) == "values.yaml";
                candidates.push(ValuesSource {
                    rank: if is_defaults { rank as u32 } else { 100 },
                    label: if is_defaults {
                        if rank == 0 {
                            format!("chart defaults ({})", file_name(dir))
                        } else {
                            format!("parent chart values ({})", file_name(dir))
                        }
                    } else {
                        format!("user-supplied -f {}", file_name(&file))
                    },
                    file,
                    symbol,
                });
            }
        }
        if candidates.len() < 2 && !candidates.iter().any(|c| c.rank == 100) {
            // A single source is not a competition; the key stands as written.
            self.stop(
                depth,
                StopReason::ExternalInput {
                    name: format!("values key {}", local.join(".")),
                    required: false,
                    sources: "`-f` files and `--set` on the helm command line".to_string(),
                },
            );
            return Ok(false);
        }

        candidates.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| file_name(&a.file).cmp(&file_name(&b.file)))
        });
        let top_rank = candidates.last().map(|c| c.rank).unwrap_or_default();
        let contested = candidates.iter().filter(|c| c.rank == top_rank).count() > 1;
        let winner = candidates.len() - 1;

        let mut sources = Vec::new();
        for (i, candidate) in candidates.iter().enumerate() {
            let text = self.source(&candidate.file)?;
            let symbol = self.index.symbol(candidate.symbol).expect("key exists");
            let hop = self.hop(
                Some(candidate.symbol),
                EdgeKind::Override,
                snippet(symbol.full_span.text(&text)),
                &candidate.file,
                symbol.full_span,
                depth + 1,
            )?;
            sources.push(CompetingSource {
                hop,
                precedence: Precedence::level(candidate.rank, candidate.label.clone()),
                wins: i == winner && !contested,
                reason: if contested && candidate.rank == top_rank {
                    "ties with another source at the same level; -f order decides".to_string()
                } else if i == winner {
                    "highest-precedence source visible in the workspace".to_string()
                } else {
                    format!("overridden by {}", candidates[winner].label)
                },
            });
        }
        sources.reverse();
        for source in &sources {
            self.push_hop(source.hop.clone());
        }
        if contested {
            self.stop(
                depth,
                StopReason::PrecedenceUndetermined(format!(
                    "several user-supplied values files set {}; their order is the order of -f on the command line",
                    local.join(".")
                )),
            );
        }
        self.out.competitions.push(Competition {
            subject: format!("values key {}", local.join(".")),
            model: "helm: subchart values.yaml < parent chart values.yaml < user-supplied -f files < --set"
                .to_string(),
            decided: false,
            sources,
        });
        self.stop(
            depth,
            StopReason::ExternalInput {
                name: format!("values key {}", local.join(".")),
                required: false,
                sources: "`-f` files and `--set` on the helm command line".to_string(),
            },
        );
        Ok(true)
    }

    /// The dotted key path of a mapping key, read off the container chain.
    ///
    /// Keys under a sequence are qualified by the sequence's key, so `ports[0].port`
    /// reads as `ports.port`: the index records no sequence indices.
    fn key_path(&self, sym: &Symbol) -> Vec<String> {
        let mut path = vec![sym.name.clone()];
        let mut current = sym;
        while let Some(id) = current.container {
            let Some(parent) = self.index.symbol(id) else {
                break;
            };
            path.push(parent.name.clone());
            current = parent;
        }
        path.reverse();
        path
    }

    fn find_key(&self, file: &Path, path: &[String]) -> Option<SymbolId> {
        let info = self.index.file(file)?;
        info.symbols
            .iter()
            .filter_map(|id| self.index.symbol(*id))
            .find(|s| s.kind == SymbolKind::Key && self.key_path(s) == path)
            .map(|s| s.id)
    }

    /// Walk a values path down through subcharts: in a parent chart, the key
    /// `mysql.image.tag` addresses subchart `mysql`'s own `image.tag`.
    fn descend_to_subchart(&self, chart: &Path, path: &[String]) -> (PathBuf, Vec<String>) {
        let mut chart = chart.to_path_buf();
        let mut path = path.to_vec();
        while !path.is_empty() {
            let child = chart.join("charts").join(&path[0]);
            if !has_chart_yaml(&child) {
                break;
            }
            chart = child;
            path.remove(0);
        }
        (chart, path)
    }

    /// Forward: which templates read this values key, and what overrides it?
    fn yaml_forward(&mut self, sym: &Symbol, depth: usize) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        if !self.seen.insert(sym.id) {
            return Ok(());
        }
        let source = self.source(&sym.file)?;
        let hop = self.hop(
            Some(sym.id),
            EdgeKind::Declaration,
            snippet(sym.full_span.text(&source)),
            &sym.file,
            sym.full_span,
            depth,
        )?;
        self.push_hop(hop);

        // An anchor's consumers are its aliases, which the index resolves.
        if sym.kind == SymbolKind::Anchor {
            let uses: Vec<(PathBuf, Span, String)> = self
                .index
                .references_to(sym.id)
                .into_iter()
                .map(|r| (r.file.clone(), r.span, format!("*{}", r.name)))
                .collect();
            if uses.is_empty() {
                self.stop(
                    depth,
                    StopReason::Origin(format!("anchor &{} has no aliases", sym.name)),
                );
            }
            for (file, span, text) in uses {
                let hop = self.hop(
                    None,
                    EdgeKind::Expansion,
                    text,
                    &file,
                    span,
                    depth + 1,
                )?;
                self.push_hop(hop);
            }
            return Ok(());
        }

        self.helm_values_competition(sym, depth)?;

        // Templates read values through masked actions; the link is textual.
        let (chart, path) = match chart_root(&sym.file) {
            Some(chart) => {
                let path = self.key_path(sym);
                let (owner, local) = self.descend_to_subchart(&chart, &path);
                (owner, local)
            }
            None => {
                self.stop(
                    depth,
                    StopReason::Origin(format!(
                        "{} is not part of a chart; nothing in the workspace consumes it",
                        short(&sym.file)
                    )),
                );
                return Ok(());
            }
        };
        let dotted = path.join(".");

        let templates: Vec<PathBuf> = self
            .index
            .files()
            .filter(|(p, info)| {
                info.language == Language::Helm && p.starts_with(chart.join("templates"))
            })
            .map(|(p, _)| p.clone())
            .collect();
        let mut readers = 0;
        for template in templates {
            let text = self.source(&template)?;
            let parsed = Parsers::new().parse(Language::Helm, &text)?;
            for action in parsed.template_actions.clone() {
                let action_text = action.text(&text).to_string();
                if !values_paths_in(&action_text)
                    .iter()
                    .any(|p| p.join(".") == dotted)
                {
                    continue;
                }
                readers += 1;
                let hop = Hop {
                    confidence: Confidence::NameOnly,
                    ..self.hop(
                        None,
                        EdgeKind::TemplateAction,
                        action_text.clone(),
                        &template,
                        action,
                        depth + 1,
                    )?
                };
                self.push_hop(hop);
                self.stop(depth + 1, StopReason::RenderDependent(action_text));
            }
        }
        if readers == 0 {
            self.stop(
                depth,
                StopReason::Origin(format!(
                    "no template action in {} names .Values.{dotted}",
                    short(&chart)
                )),
            );
        }
        Ok(())
    }
}

/// Every `.Values.a.b.c` path named inside a template action.
fn values_paths_in(action: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let bytes = action.as_bytes();
    let needle = b".Values.";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }
        let mut j = i + needle.len();
        while j < bytes.len()
            && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-' || bytes[j] == b'.')
        {
            j += 1;
        }
        let path: Vec<String> = action[i + needle.len()..j]
            .trim_end_matches('.')
            .split('.')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if !path.is_empty() {
            out.push(path);
        }
        i = j.max(i + 1);
    }
    out.dedup();
    out
}

/// Helm's built-in objects named inside a template action. Their values come from
/// the release, not from any file in the workspace.
fn builtins_in(action: &str) -> Vec<String> {
    ["Release", "Chart", "Capabilities", "Template", "Files"]
        .iter()
        .filter(|name| action.contains(&format!(".{name}.")))
        .map(|name| format!(".{name}"))
        .collect()
}

/// The scalar text a `key: value` pair binds, when the pair holds one.
fn key_value_text(source: &str, sym: &Symbol) -> Option<String> {
    let full = sym.full_span.text(source);
    let colon = full.find(':')?;
    let value = full[colon + 1..].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn is_values_file(path: &Path) -> bool {
    let name = file_name(path);
    name.starts_with("values")
        && (name.ends_with(".yaml") || name.ends_with(".yml"))
}

fn has_chart_yaml(dir: &Path) -> bool {
    dir.join("Chart.yaml").exists() || dir.join("chart.yaml").exists()
}

/// The nearest ancestor directory holding a `Chart.yaml`.
fn chart_root(file: &Path) -> Option<PathBuf> {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if has_chart_yaml(current) {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// A chart and each chart that encloses it, with the key path as addressed at that
/// level. Index 0 is the chart itself (lowest precedence).
fn chart_levels(chart: &Path, local: &[String]) -> Vec<(PathBuf, Vec<String>)> {
    let mut levels = vec![(chart.to_path_buf(), local.to_vec())];
    let mut current = chart.to_path_buf();
    let mut path = local.to_vec();
    while let Some(charts_dir) = current.parent() {
        if file_name(charts_dir) != "charts" {
            break;
        }
        let Some(parent) = charts_dir.parent() else {
            break;
        };
        if !has_chart_yaml(parent) {
            break;
        }
        let mut prefixed = vec![file_name(&current)];
        prefixed.extend(path.iter().cloned());
        path = prefixed;
        current = parent.to_path_buf();
        levels.push((current.clone(), path.clone()));
    }
    levels
}

// ----------------------------------------------------------------------- CSS

/// One declaration competing in the cascade.
#[derive(Debug, Clone)]
struct CssDeclaration {
    property: String,
    file: PathBuf,
    span: Span,
    value: Span,
    text: String,
    selector: String,
    specificity: Specificity,
    important: bool,
    layer: Option<String>,
    /// `@media`/`@supports`/`@container` preludes the declaration sits inside.
    conditions: Vec<String>,
}

impl Ctx<'_> {
    fn css_backward(&mut self, sym: &Symbol, depth: usize) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        let source = self.source(&sym.file)?;
        let hop = self.hop(
            Some(sym.id),
            EdgeKind::Declaration,
            snippet(sym.full_span.text(&source)),
            &sym.file,
            sym.full_span,
            depth,
        )?;
        self.push_hop(hop);

        match sym.kind {
            SymbolKind::Property => self.css_property(&sym.name, depth, Some(sym.id)),
            SymbolKind::Selector | SymbolKind::ElementId => self.css_selector(sym, depth),
            other => {
                self.stop(
                    depth,
                    StopReason::NotAValue(format!("{} '{}'", other.as_str(), sym.name)),
                );
                Ok(())
            }
        }
    }

    /// Resolve a custom property through the cascade, then follow its `var()` chain.
    fn css_property(&mut self, name: &str, depth: usize, symbol: Option<SymbolId>) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        if let Some(id) = symbol {
            if !self.seen.insert(id) {
                return Ok(());
            }
        }
        let declarations = self.css_declarations_of(name)?;
        if declarations.is_empty() {
            self.stop(depth, StopReason::Unresolved(format!("var({name})")));
            return Ok(());
        }

        let winner = self.css_competition(
            format!("custom property {name}"),
            declarations.clone(),
            depth,
        )?;
        let Some(winner) = winner else {
            return Ok(());
        };

        // The winning declaration's own value may itself be a var() chain.
        let declaration = &declarations[winner];
        let references: Vec<String> = self
            .refs_in(&declaration.file, declaration.value)
            .into_iter()
            .filter(|r| r.name.starts_with("--"))
            .map(|r| r.name.clone())
            .collect();
        if references.is_empty() {
            self.stop(
                depth,
                StopReason::Origin(format!("literal value {}", declaration.text)),
            );
            return Ok(());
        }
        for reference in references {
            let hop = {
                let text = format!("var({reference})");
                let file = declaration.file.clone();
                let span = declaration.value;
                self.hop(
                    None,
                    EdgeKind::VarFunction,
                    text,
                    &file,
                    span,
                    depth + 1,
                )?
            };
            self.push_hop(hop);
            self.css_property(&reference, depth + 1, None)?;
        }
        Ok(())
    }

    /// Every declaration that a rule naming this selector makes, grouped by property.
    fn css_selector(&mut self, sym: &Symbol, depth: usize) -> Result<()> {
        let sites = self.index.definition_group(sym.id);
        let mut declarations: Vec<CssDeclaration> = Vec::new();
        for site in sites {
            let Some(symbol) = self.index.symbol(site) else {
                continue;
            };
            let source = self.source(&symbol.file)?;
            declarations.extend(css_rule_declarations(&source, &symbol.file, symbol.name_span)?);
        }
        if declarations.is_empty() {
            self.stop(
                depth,
                StopReason::NotAValue(format!("selector '{}' declares nothing", sym.name)),
            );
            return Ok(());
        }

        let mut properties: Vec<String> = declarations.iter().map(|d| d.property.clone()).collect();
        properties.sort();
        properties.dedup();
        for property in properties {
            let group: Vec<CssDeclaration> = declarations
                .iter()
                .filter(|d| d.property == property)
                .cloned()
                .collect();
            self.css_competition(
                format!("{property} on '{}'", sym.name),
                group,
                depth,
            )?;
        }
        Ok(())
    }

    /// Every declaration of `property` anywhere in the workspace's stylesheets.
    fn css_declarations_of(&mut self, property: &str) -> Result<Vec<CssDeclaration>> {
        let sites: Vec<(PathBuf, Span)> = self
            .index
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property && s.name == property)
            .map(|s| (s.file.clone(), s.name_span))
            .collect();
        let mut out = Vec::new();
        for (file, span) in sites {
            let source = self.source(&file)?;
            out.extend(css_declaration_at(&source, &file, span)?);
        }
        Ok(out)
    }

    /// Run the cascade over competing declarations, marking the winner and keeping
    /// every loser. Returns the winning index, if the cascade decides one.
    fn css_competition(
        &mut self,
        subject: String,
        declarations: Vec<CssDeclaration>,
        depth: usize,
    ) -> Result<Option<usize>> {
        let (winner, undetermined) = css_winner(&declarations);
        let conditional: Vec<String> = declarations
            .iter()
            .flat_map(|d| d.conditions.clone())
            .collect();

        let mut order: Vec<usize> = (0..declarations.len()).collect();
        order.sort_by(|a, b| match css_compare(&declarations[*a], &declarations[*b]) {
            Some(ordering) => ordering.reverse(),
            None => Ordering::Equal,
        });

        let mut sources = Vec::new();
        for index in order {
            let declaration = &declarations[index];
            let hop = self.hop(
                None,
                EdgeKind::Override,
                declaration.text.clone(),
                &declaration.file,
                declaration.span,
                depth + 1,
            )?;
            let wins = winner == Some(index);
            let mut reason = if wins {
                format!(
                    "wins: specificity {}{}",
                    declaration.specificity,
                    if declaration.important {
                        ", !important"
                    } else {
                        ""
                    }
                )
            } else if undetermined.is_some() {
                format!("specificity {}", declaration.specificity)
            } else {
                let best = &declarations[winner.expect("a winner exists")];
                format!(
                    "loses to '{}' {} at {}:{}",
                    best.selector,
                    best.specificity,
                    short(&best.file),
                    best.text
                )
            };
            if !declaration.conditions.is_empty() {
                reason.push_str(&format!(
                    "; applies only when {} matches",
                    declaration.conditions.join(" and ")
                ));
            }
            sources.push(CompetingSource {
                hop,
                precedence: Precedence {
                    rank: u32::from(declaration.important),
                    label: format!("author stylesheet, selector '{}'", declaration.selector),
                    specificity: Some(declaration.specificity),
                    important: declaration.important,
                    layer: declaration.layer.clone(),
                },
                wins,
                reason,
            });
        }
        for source in &sources {
            self.push_hop(source.hop.clone());
        }
        if let Some(reason) = &undetermined {
            self.stop(depth, StopReason::PrecedenceUndetermined(reason.clone()));
        }
        if !conditional.is_empty() {
            self.stop(
                depth,
                StopReason::PrecedenceUndetermined(format!(
                    "a competing declaration is conditional on {}, which the stylesheet alone cannot decide",
                    conditional.join(" and ")
                )),
            );
        }
        self.out.competitions.push(Competition {
            subject,
            model: "css cascade: origin → !important → layer → specificity → source order"
                .to_string(),
            decided: undetermined.is_none() && conditional.is_empty(),
            sources,
        });
        Ok(winner)
    }

    /// Forward: what reads this custom property or selector?
    fn css_forward(&mut self, sym: &Symbol, depth: usize) -> Result<()> {
        if self.over_depth(depth) {
            return Ok(());
        }
        if !self.seen.insert(sym.id) {
            return Ok(());
        }
        let source = self.source(&sym.file)?;
        let hop = self.hop(
            Some(sym.id),
            EdgeKind::Declaration,
            snippet(sym.full_span.text(&source)),
            &sym.file,
            sym.full_span,
            depth,
        )?;
        self.push_hop(hop);

        let group = self.index.definition_group(sym.id);
        let uses: Vec<(PathBuf, Span, String, Confidence)> = self
            .index
            .references
            .iter()
            .filter(|r| group.iter().any(|id| r.target == Some(*id)))
            .map(|r| (r.file.clone(), r.span, r.name.clone(), r.confidence))
            .collect();
        if uses.is_empty() {
            self.stop(
                depth,
                StopReason::Origin(format!("'{}' is read nowhere in the workspace", sym.name)),
            );
            return Ok(());
        }
        for (file, span, name, confidence) in uses {
            let text = self.source(&file)?;
            let lines = LineIndex::new(&text);
            let line = lines.line_col(span.start, &text).line;
            let line_text = lines
                .line_span(line)
                .map(|s| s.text(&text).trim().to_string())
                .unwrap_or(name);
            let kind = if sym.kind == SymbolKind::Property {
                EdgeKind::VarFunction
            } else {
                EdgeKind::Use
            };
            let hop = Hop {
                confidence,
                ..self.hop(None, kind, line_text, &file, span, depth + 1)?
            };
            self.push_hop(hop);

            // A custom property read inside another declaration carries onward.
            if sym.kind == SymbolKind::Property {
                if let Some(declaration) = css_declaration_containing(&text, &file, span)? {
                    if declaration.property.starts_with("--") {
                        let onward: Vec<SymbolId> = self
                            .index
                            .symbols
                            .iter()
                            .filter(|s| {
                                s.kind == SymbolKind::Property && s.name == declaration.property
                            })
                            .map(|s| s.id)
                            .collect();
                        for id in onward {
                            let next = self.index.symbol(id).expect("property exists");
                            self.css_forward(next, depth + 2)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Compare two declarations by the cascade. `None` means the workspace does not
/// show which wins.
fn css_compare(a: &CssDeclaration, b: &CssDeclaration) -> Option<Ordering> {
    if a.important != b.important {
        return Some(a.important.cmp(&b.important));
    }
    // Unlayered author declarations beat layered ones; `!important` reverses it.
    match (&a.layer, &b.layer) {
        (None, None) => {}
        (None, Some(_)) => return Some(if a.important { Ordering::Less } else { Ordering::Greater }),
        (Some(_), None) => return Some(if a.important { Ordering::Greater } else { Ordering::Less }),
        (Some(x), Some(y)) if x != y => return None,
        (Some(_), Some(_)) => {}
    }
    if a.specificity != b.specificity {
        return Some(a.specificity.cmp(&b.specificity));
    }
    if a.file == b.file {
        return Some(a.span.start.cmp(&b.span.start));
    }
    None
}

/// The winning declaration, or the reason there is no determinable winner.
fn css_winner(declarations: &[CssDeclaration]) -> (Option<usize>, Option<String>) {
    if declarations.is_empty() {
        return (None, None);
    }
    let mut best = 0usize;
    for candidate in 1..declarations.len() {
        match css_compare(&declarations[candidate], &declarations[best]) {
            Some(Ordering::Greater) => best = candidate,
            Some(_) => {}
            None => {}
        }
    }
    for (index, declaration) in declarations.iter().enumerate() {
        if index == best {
            continue;
        }
        match css_compare(&declarations[best], declaration) {
            Some(Ordering::Greater) => {}
            Some(_) => {
                return (
                    None,
                    Some(format!(
                        "'{}' and '{}' declare {} with the same weight",
                        declarations[best].selector, declaration.selector, declaration.property
                    )),
                )
            }
            None => {
                let reason = match (&declarations[best].layer, &declaration.layer) {
                    (Some(x), Some(y)) if x != y => format!(
                        "'{}' is in @layer {x} and '{}' in @layer {y}; layer order beats specificity and is not visible here",
                        declarations[best].selector, declaration.selector
                    ),
                    _ => format!(
                        "'{}' ({}) and '{}' ({}) live in different stylesheets, so source order depends on load order",
                        declarations[best].selector,
                        short(&declarations[best].file),
                        declaration.selector,
                        short(&declaration.file)
                    ),
                };
                return (None, Some(reason));
            }
        }
    }
    (Some(best), None)
}

/// Every declaration of the rule whose selector contains `selector_span`.
fn css_rule_declarations(
    source: &str,
    file: &Path,
    selector_span: Span,
) -> Result<Vec<CssDeclaration>> {
    let parsed = Parsers::new().parse(css_language(file), source)?;
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(selector_span.start, selector_span.end)
    else {
        return Ok(Vec::new());
    };
    let Some(rule) = ancestor_of_kind(node, "rule_set") else {
        return Ok(Vec::new());
    };
    // The specificity is that of the one selector in the list that names us.
    let selector = ancestor_selector(node, rule);
    let selector_text = selector
        .map(|s| Span::from(s).text(source).to_string())
        .unwrap_or_else(|| selector_list_text(rule, source));
    let (layer, conditions) = css_context(rule, source);

    let Some(block) = child_of_kind(rule, "block") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut cursor = block.walk();
    for child in block.named_children(&mut cursor) {
        if child.kind() != "declaration" {
            continue;
        }
        if let Some(declaration) = css_declaration_from(
            child,
            source,
            file,
            &selector_text,
            layer.clone(),
            conditions.clone(),
        ) {
            out.push(declaration);
        }
    }
    Ok(out)
}

/// The declaration whose property name is at `span`.
fn css_declaration_at(source: &str, file: &Path, span: Span) -> Result<Vec<CssDeclaration>> {
    let parsed = Parsers::new().parse(css_language(file), source)?;
    let Some(node) = parsed.root().descendant_for_byte_range(span.start, span.end) else {
        return Ok(Vec::new());
    };
    let Some(declaration) = ancestor_of_kind(node, "declaration") else {
        return Ok(Vec::new());
    };
    Ok(css_declaration_context(declaration, source, file)
        .into_iter()
        .collect())
}

/// The declaration containing an arbitrary span, e.g. a `var()` use site.
fn css_declaration_containing(
    source: &str,
    file: &Path,
    span: Span,
) -> Result<Option<CssDeclaration>> {
    let parsed = Parsers::new().parse(css_language(file), source)?;
    let Some(node) = parsed.root().descendant_for_byte_range(span.start, span.end) else {
        return Ok(None);
    };
    let Some(declaration) = ancestor_of_kind(node, "declaration") else {
        return Ok(None);
    };
    Ok(css_declaration_context(declaration, source, file))
}

fn css_declaration_context(
    declaration: Node<'_>,
    source: &str,
    file: &Path,
) -> Option<CssDeclaration> {
    let rule = ancestor_of_kind(declaration, "rule_set");
    let selector_text = rule
        .map(|r| selector_list_text(r, source))
        .unwrap_or_else(|| "<no rule>".to_string());
    let (layer, conditions) = css_context(rule.unwrap_or(declaration), source);
    css_declaration_from(declaration, source, file, &selector_text, layer, conditions)
}

fn css_declaration_from(
    declaration: Node<'_>,
    source: &str,
    file: &Path,
    selector: &str,
    layer: Option<String>,
    conditions: Vec<String>,
) -> Option<CssDeclaration> {
    let property = child_of_kind(declaration, "property_name")?;
    let mut important = false;
    let mut value: Option<Span> = None;
    let mut cursor = declaration.walk();
    for child in declaration.named_children(&mut cursor) {
        match child.kind() {
            "property_name" => {}
            "important" => important = true,
            _ => {
                let span = Span::from(child);
                value = Some(match value {
                    Some(current) => Span::new(current.start, span.end),
                    None => span,
                });
            }
        }
    }
    Some(CssDeclaration {
        property: Span::from(property).text(source).to_string(),
        file: file.to_path_buf(),
        span: Span::from(declaration),
        value: value.unwrap_or_else(|| Span::from(declaration)),
        text: snippet(Span::from(declaration).text(source)),
        selector: selector.to_string(),
        specificity: specificity(selector),
        important,
        layer,
        conditions,
    })
}

/// The `@layer` a node sits in, plus any conditional at-rules around it.
fn css_context(node: Node<'_>, source: &str) -> (Option<String>, Vec<String>) {
    let mut layer = None;
    let mut conditions = Vec::new();
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "at_rule" || parent.kind() == "media_statement" {
            let text = Span::from(parent).text(source);
            let head = text.lines().next().unwrap_or("").trim_end_matches('{').trim();
            if head.starts_with("@layer") {
                layer = Some(head.trim_start_matches("@layer").trim().to_string());
            } else if head.starts_with("@media")
                || head.starts_with("@supports")
                || head.starts_with("@container")
            {
                conditions.push(head.to_string());
            }
        }
        current = parent.parent();
    }
    (layer, conditions)
}

/// The individual selector inside a selector list that contains `node`.
fn ancestor_selector<'t>(node: Node<'t>, rule: Node<'t>) -> Option<Node<'t>> {
    let selectors = child_of_kind(rule, "selectors")?;
    let mut cursor = selectors.walk();
    let found = selectors
        .named_children(&mut cursor)
        .find(|child| Span::from(*child).contains(Span::from(node)));
    found
}

fn selector_list_text(rule: Node<'_>, source: &str) -> String {
    child_of_kind(rule, "selectors")
        .map(|s| Span::from(s).text(source).to_string())
        .unwrap_or_default()
}

fn css_language(file: &Path) -> Language {
    match file.extension().and_then(|e| e.to_str()) {
        Some("scss") | Some("sass") => Language::Scss,
        _ => Language::Css,
    }
}

/// CSS specificity: (id, class/attribute/pseudo-class, element/pseudo-element).
///
/// Implements the spec's counting rules, including `:is()`/`:not()`/`:has()`
/// taking their most specific argument and `:where()` contributing nothing.
pub fn specificity(selector: &str) -> Specificity {
    // A selector list has no single specificity; the strongest branch is the one
    // that can win, so report that. Only a comma outside parentheses splits a
    // list — the one in `:is(#a, .b)` belongs to the functional pseudo-class.
    let branches = split_top_level(selector, ',');
    if branches.len() > 1 {
        return branches
            .into_iter()
            .map(|part| specificity(&part))
            .max()
            .unwrap_or_default();
    }

    let mut result = Specificity::default();
    let bytes: Vec<char> = selector.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            '#' => {
                result.ids += 1;
                i += 1 + ident_len(&bytes[i + 1..]);
            }
            '.' => {
                result.classes += 1;
                i += 1 + ident_len(&bytes[i + 1..]);
            }
            '[' => {
                result.classes += 1;
                i += bytes[i..]
                    .iter()
                    .position(|c| *c == ']')
                    .map(|p| p + 1)
                    .unwrap_or(bytes.len() - i);
            }
            ':' => {
                let double = bytes.get(i + 1) == Some(&':');
                let start = i + 1 + usize::from(double);
                let len = ident_len(&bytes[start..]);
                let name: String = bytes[start..start + len].iter().collect();
                i = start + len;
                let argument = if bytes.get(i) == Some(&'(') {
                    let (text, consumed) = balanced(&bytes[i..]);
                    i += consumed;
                    Some(text)
                } else {
                    None
                };
                let lowered = name.to_ascii_lowercase();
                if double || matches!(lowered.as_str(), "before" | "after" | "first-line" | "first-letter") {
                    result.elements += 1;
                } else if lowered == "where" {
                    // Contributes nothing, by design.
                } else if matches!(lowered.as_str(), "is" | "not" | "has" | "matches" | "any") {
                    if let Some(inner) = argument {
                        let inner = specificity(&inner);
                        result.ids += inner.ids;
                        result.classes += inner.classes;
                        result.elements += inner.elements;
                    }
                } else {
                    result.classes += 1;
                }
            }
            '*' | ' ' | '>' | '+' | '~' | ')' | '(' => i += 1,
            c if is_ident_char(c) => {
                let len = ident_len(&bytes[i..]);
                // `ns|element`: the namespace prefix is not counted, the element is.
                if bytes.get(i + len) == Some(&'|') && bytes.get(i + len + 1) != Some(&'|') {
                    i += len + 1;
                    continue;
                }
                result.elements += 1;
                i += len;
            }
            _ => i += 1,
        }
    }
    result
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_' || c == '\\'
}

fn ident_len(rest: &[char]) -> usize {
    rest.iter().take_while(|c| is_ident_char(**c)).count()
}

/// The contents of a parenthesised group, and how many chars it spanned.
fn balanced(rest: &[char]) -> (String, usize) {
    let mut depth = 0usize;
    let mut text = String::new();
    for (i, c) in rest.iter().enumerate() {
        match c {
            '(' => {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (text, i + 1);
                }
            }
            _ => {}
        }
        text.push(*c);
    }
    (text, rest.len())
}

fn split_top_level(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for c in text.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c == separator && depth == 0 {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

// -------------------------------------------------------------------- shared

/// The namespace segment written immediately before a reference: the `var` of
/// `var.region`, the `module` of `module.network.vpc_id`.
fn namespace_before(source: &str, span: Span) -> Option<String> {
    let bytes = source.as_bytes();
    if span.start == 0 || bytes[span.start - 1] != b'.' {
        return None;
    }
    let end = span.start - 1;
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    (start < end).then(|| source[start..end].to_string())
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
    found
}

fn ancestor_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate.kind() == kind {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

/// One line of an expression, marked when there is more of it.
fn snippet(text: &str) -> String {
    let trimmed = text.trim();
    let first = trimmed.lines().next().unwrap_or("").trim();
    if trimmed.lines().count() > 1 {
        format!("{first} …")
    } else {
        first.to_string()
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// The last two path components, for readable messages.
fn short(path: &Path) -> String {
    let mut parts: Vec<String> = path
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    parts.reverse();
    parts.join("/")
}

/// Resolve `.` and `..` without touching the filesystem, so module sources like
/// `./modules/network` compare equal to the directory the index holds.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specificity_follows_the_spec_counting_rules() {
        assert_eq!(specificity("*"), Specificity { ids: 0, classes: 0, elements: 0 });
        assert_eq!(specificity("li"), Specificity { ids: 0, classes: 0, elements: 1 });
        assert_eq!(specificity(".btn"), Specificity { ids: 0, classes: 1, elements: 0 });
        assert_eq!(specificity("#main"), Specificity { ids: 1, classes: 0, elements: 0 });
        assert_eq!(
            specificity("#main .btn:hover"),
            Specificity { ids: 1, classes: 2, elements: 0 }
        );
        assert_eq!(
            specificity("a[href^=\"http\"]"),
            Specificity { ids: 0, classes: 1, elements: 1 }
        );
        assert_eq!(
            specificity("p::before"),
            Specificity { ids: 0, classes: 0, elements: 2 }
        );
        // Legacy single-colon pseudo-elements count as elements, not classes.
        assert_eq!(specificity("p:before"), Specificity { ids: 0, classes: 0, elements: 2 });
    }

    #[test]
    fn functional_pseudo_classes_take_their_argument() {
        // `:not()`/`:is()` take their most specific argument; `:where()` takes none.
        assert_eq!(
            specificity(".card:not(.hidden)"),
            Specificity { ids: 0, classes: 2, elements: 0 }
        );
        assert_eq!(
            specificity(":is(#a, .b)"),
            Specificity { ids: 1, classes: 0, elements: 0 }
        );
        assert_eq!(
            specificity(":where(#a) .b"),
            Specificity { ids: 0, classes: 1, elements: 0 }
        );
    }

    #[test]
    fn a_selector_list_reports_its_strongest_branch() {
        assert_eq!(
            specificity(".a, #b"),
            Specificity { ids: 1, classes: 0, elements: 0 }
        );
    }

    #[test]
    fn namespace_prefix_is_not_counted_as_an_element() {
        assert_eq!(
            specificity("svg|circle"),
            Specificity { ids: 0, classes: 0, elements: 1 }
        );
    }

    #[test]
    fn terraform_namespaces_are_read_off_the_source() {
        let src = "value = var.region";
        let span = Span::new(src.find("region").unwrap(), src.len());
        assert_eq!(namespace_before(src, span).as_deref(), Some("var"));
        // A bare identifier has no namespace.
        let bare = "value = region";
        let span = Span::new(bare.find("region").unwrap(), bare.len());
        assert_eq!(namespace_before(bare, span), None);
    }

    #[test]
    fn values_paths_are_extracted_from_a_masked_action() {
        let paths = values_paths_in("{{ .Values.image.repository }}:{{ .Values.image.tag }}");
        assert_eq!(
            paths,
            vec![
                vec!["image".to_string(), "repository".to_string()],
                vec!["image".to_string(), "tag".to_string()]
            ]
        );
        // Pipelines and defaults do not corrupt the path.
        assert_eq!(
            values_paths_in("{{ .Values.replicaCount | default 3 }}"),
            vec![vec!["replicaCount".to_string()]]
        );
    }

    #[test]
    fn helm_builtins_are_recognised_as_external() {
        assert_eq!(builtins_in("{{ .Release.Name }}"), vec![".Release"]);
        assert!(builtins_in("{{ .Values.x }}").is_empty());
    }
}
