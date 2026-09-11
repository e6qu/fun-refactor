use crate::index::{content_hash_of, Index};
use crate::model::{Confidence, SymbolId};
use crate::parse::{Parsed, Parsers};
use crate::scan::{scan, ScanOptions, ScanResult};
use crate::span::{LineIndex, Span};
use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};
use digest::RevisionDigest;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub mod author;
mod components;
mod configuration;
mod context;
mod contracts;
mod digest;
#[cfg(test)]
mod digest_tests;
mod fast_routes;
mod features;
mod find;
pub use crate::framework_kernel;
mod links;
mod manifests;
pub mod migration;
mod next_routes;
mod relationships;
mod routes;
mod schemas;
mod service_calls;
mod tests;

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Find declaration handles by literal name without loading file maps.")]
    Find(find::Options),
    #[command(about = "Find several exact declaration names in one revision-bound query.")]
    Select(find::SelectOptions),
    #[command(about = "Page through a directory, file or symbol hierarchy.")]
    Map {
        #[arg(default_value = ".", help = "Workspace path or revision-bound handle")]
        target: String,
        #[arg(long, help = "Required when TARGET is a short node ID.")]
        revision: Option<String>,
        #[arg(
            long,
            default_value_t = 3,
            help = "Containment levels below the selected root."
        )]
        depth: usize,
        #[arg(long, help = "Include variables and parameters")]
        locals: bool,
        #[arg(long, default_value_t = 80)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "id,parent,kind,name,line,children"
        )]
        fields: Vec<Field>,
    },
    #[command(about = "Inspect a handle, with optional source and reference pages.")]
    Show(ShowOptions),
    #[command(about = "Page through call sites, dispatch candidates and unresolved calls.")]
    Calls {
        #[command(flatten)]
        selection: RelationshipOptions,
        #[arg(long, value_enum, default_value = "both")]
        direction: CallDirection,
    },
    #[command(about = "Page through implementation candidates for selected declarations.")]
    Implementations(RelationshipOptions),
    #[command(about = "Page through route declaration patterns and local handler candidates.")]
    Routes(RelationshipOptions),
    #[command(about = "Page through partial route request and response contract candidates.")]
    Contracts {
        #[command(flatten)]
        selection: RelationshipOptions,
        #[arg(
            long,
            help = "Include paged type references from supported handler signatures."
        )]
        types: bool,
    },
    #[command(about = "Page through route and page centered application feature hierarchies.")]
    Features(FeatureOptions),
    #[command(about = "Page through declared schema fields and local type-reference candidates.")]
    Schemas(RelationshipOptions),
    #[command(about = "Page through environment declarations and candidate code consumers.")]
    Configuration(RelationshipOptions),
    #[command(about = "Page through test candidates and call-path witnesses.")]
    Tests {
        #[command(flatten)]
        selection: RelationshipOptions,
        #[arg(
            long,
            default_value_t = 3,
            help = "Maximum call-path length, from 0 through 16."
        )]
        depth: usize,
    },
    #[command(about = "Page through Cargo and npm package manifest boundaries.")]
    Packages {
        #[arg(long, default_value_t = 40)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
    #[command(about = "Page through dependency declarations and workspace patterns.")]
    Dependencies {
        #[arg(long, help = "Select a manifest path relative to the project root.")]
        manifest: Option<PathBuf>,
        #[arg(long, default_value_t = 40)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
    #[command(about = "Page through local manifest links and workspace pattern matches.")]
    Links {
        #[arg(long, help = "Select a manifest path relative to the project root.")]
        manifest: Option<PathBuf>,
        #[arg(long, default_value_t = 40)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
    #[command(about = "Page through observed Cargo workspace ownership and membership.")]
    Workspaces {
        #[arg(long, help = "Select a manifest path relative to the project root.")]
        manifest: Option<PathBuf>,
        #[arg(long, default_value_t = 40)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
    #[command(about = "Page through skipped files and incomplete facts.")]
    Gaps {
        #[arg(long, default_value_t = 40)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
}

#[derive(clap::Args)]
pub struct FeatureOptions {
    #[command(flatten)]
    selection: RelationshipOptions,
    #[arg(
        long,
        help = "Select one feature ID returned by this project revision."
    )]
    feature: Option<String>,
}

#[derive(clap::Args)]
pub struct RelationshipOptions {
    #[arg(default_value = ".", help = "Workspace path or revision-bound handle.")]
    target: String,
    #[arg(long, help = "Required when TARGET is a short node ID.")]
    revision: Option<String>,
    #[arg(long, default_value_t = 40)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CallDirection {
    Incoming,
    Outgoing,
    Both,
}

#[derive(clap::Args)]
pub struct ShowOptions {
    handle: String,
    #[arg(long, help = "Required when HANDLE is a short node ID.")]
    revision: Option<String>,
    #[arg(long, help = "Include a bounded source slice")]
    source: bool,
    #[arg(long, default_value_t = 0, requires = "source")]
    offset: usize,
    #[arg(long, default_value_t = 2048)]
    bytes: usize,
    #[arg(
        long,
        help = "Include file imports and incoming/outgoing indexed references."
    )]
    relations: bool,
    #[arg(long, default_value_t = 40)]
    limit: usize,
    #[arg(long, requires = "relations")]
    cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Field {
    Id,
    Handle,
    Parent,
    Kind,
    Name,
    Path,
    Line,
    Children,
    Depth,
    Language,
    Exported,
    Signature,
    Qualifier,
}

struct Node {
    name: String,
    kind: String,
    path: PathBuf,
    parent: Option<usize>,
    children: Vec<usize>,
    symbol: Option<SymbolId>,
}

pub struct Project<'a> {
    root: PathBuf,
    index: &'a Index,
    scanned: &'a ScanResult,
    options: &'a ScanOptions,
    sources: BTreeMap<PathBuf, String>,
    lines: BTreeMap<PathBuf, LineIndex>,
    parses: RefCell<BTreeMap<PathBuf, Parsed>>,
    nodes: Vec<Node>,
    symbol_nodes: BTreeMap<SymbolId, usize>,
    revision: String,
    manifests: manifests::Manifests,
}

struct ConstructionTimer<const ENABLED: bool> {
    last: Option<std::time::Instant>,
    phases: BTreeMap<&'static str, f64>,
}

impl<const ENABLED: bool> ConstructionTimer<ENABLED> {
    fn new() -> Self {
        Self {
            last: ENABLED.then(std::time::Instant::now),
            phases: BTreeMap::new(),
        }
    }

    fn checkpoint(&mut self, name: &'static str) {
        if ENABLED {
            let now = std::time::Instant::now();
            *self.phases.entry(name).or_default() +=
                now.duration_since(self.last.unwrap()).as_secs_f64();
            self.last = Some(now);
        }
    }
}

fn hash(value: impl serde::Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?)))
}

fn bounded_text(text: &str, max: usize) -> Value {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == text.len() {
        json!(text)
    } else {
        json!({"text": &text[..end], "omitted_bytes": text.len() - end})
    }
}

fn check_limit(limit: usize) -> Result<()> {
    if !(1..=500).contains(&limit) {
        bail!("limit must be between 1 and 500");
    }
    Ok(())
}

pub fn body_replacement_budget(before: usize, after: usize) -> bool {
    (1..=65536).contains(&before) && (1..=65536).contains(&after)
}

/// Classify an exact-handle selection after resolving its revision-bound identity.
///
/// The values are part of the Rust/Lean correspondence harness: outside scope is 0,
/// a non-declaration is 1, an omitted local is 2, and a returned declaration is 3.
pub fn handle_selection_status(
    in_scope: bool,
    declaration: bool,
    is_local: bool,
    include_locals: bool,
) -> usize {
    if !in_scope {
        0
    } else if !declaration {
        1
    } else if is_local && !include_locals {
        2
    } else {
        3
    }
}

pub fn reviewed_plan_basis_allowed(complete: bool, supplied: bool, matches: bool) -> bool {
    !supplied || (complete && matches)
}

pub fn declaration_insertion_offset(prefix: &str, body_start: usize) -> usize {
    let line_start = prefix.rfind('\n').map_or(0, |at| at + 1);
    if line_start > body_start
        && prefix[line_start..]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
    {
        line_start
    } else {
        prefix.len()
    }
}

pub fn author_selection_conflict(
    left_start: usize,
    left_end: usize,
    right_start: usize,
    right_end: usize,
) -> bool {
    (left_start < right_end && right_start < left_end)
        || (left_start == left_end && right_start <= left_start && left_start <= right_end)
        || (right_start == right_end && left_start <= right_start && right_start <= left_end)
}

pub fn page_length(total: usize, start: usize, limit: usize) -> usize {
    total.saturating_sub(start).min(limit)
}

pub fn source_slice_length(text: &str, offset: usize, bytes: usize) -> Option<usize> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let mut end = offset + page_length(text.len(), offset, bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(end - offset)
}

pub fn path_confidence(edges: &[Confidence]) -> Confidence {
    edges.iter().copied().max().unwrap_or(Confidence::Exact)
}

pub fn workspace_membership_step(members: &[usize], edges: &[(usize, usize)]) -> Vec<usize> {
    let current: BTreeSet<_> = members.iter().copied().collect();
    let mut next = current.clone();
    for &(source, target) in edges {
        if current.contains(&source) {
            next.insert(target);
        }
    }
    next.into_iter().collect()
}

pub fn workspace_pattern_matches(pattern: &[String], path: &[String]) -> bool {
    pattern.len() == path.len()
        && pattern
            .iter()
            .zip(path)
            .all(|(p, part)| p == "*" || p == part)
}

fn page(
    total: usize,
    limit: usize,
    cursor: Option<&str>,
    key: &str,
) -> Result<(usize, usize, Value)> {
    check_limit(limit)?;
    let start = if let Some(cursor) = cursor {
        let (basis, offset) = cursor.rsplit_once(':').context("invalid project cursor")?;
        if basis != key {
            bail!("stale cursor or different query; restart this project query.");
        }
        offset.parse::<usize>().context("invalid cursor offset")?
    } else {
        0
    };
    if start > total {
        bail!("cursor is beyond the result set");
    }
    let end = start + page_length(total, start, limit);
    Ok((
        start,
        end,
        json!({"total": total, "returned": end - start, "before": start, "remaining": total - end,
        "next": (end < total).then(|| format!("{key}:{end}"))}),
    ))
}

impl<'a> Project<'a> {
    pub(crate) fn response_context(
        &self,
        supplied: Option<&str>,
    ) -> Result<context::ResponseContext> {
        context::ResponseContext::new(&self.revision, self.coverage(), supplied)
    }

    pub fn new(
        root: &Path,
        index: &'a Index,
        scanned: &'a ScanResult,
        options: &'a ScanOptions,
    ) -> Result<Self> {
        Self::construct::<false>(root, index, scanned, options).map(|(project, _)| project)
    }

    pub fn new_profiled(
        root: &Path,
        index: &'a Index,
        scanned: &'a ScanResult,
        options: &'a ScanOptions,
    ) -> Result<(Self, BTreeMap<&'static str, f64>)> {
        Self::construct::<true>(root, index, scanned, options)
    }

    fn construct<const PROFILE: bool>(
        root: &Path,
        index: &'a Index,
        scanned: &'a ScanResult,
        options: &'a ScanOptions,
    ) -> Result<(Self, BTreeMap<&'static str, f64>)> {
        let mut timing = ConstructionTimer::<PROFILE>::new();
        let selected = root.canonicalize()?;
        let root = if selected.is_file() {
            selected
                .parent()
                .context("file has no parent")?
                .to_path_buf()
        } else {
            selected.clone()
        };
        let manifests = manifests::Manifests::new(&selected, &root, options)?;
        timing.checkpoint("manifests");
        let mut project = Self {
            root,
            index,
            scanned,
            options,
            sources: BTreeMap::new(),
            lines: BTreeMap::new(),
            parses: RefCell::new(BTreeMap::new()),
            nodes: Vec::new(),
            symbol_nodes: BTreeMap::new(),
            revision: String::new(),
            manifests,
        };
        project.nodes.push(Node {
            name: ".".into(),
            kind: "directory".into(),
            path: PathBuf::new(),
            parent: None,
            children: Vec::new(),
            symbol: None,
        });
        let mut directories = BTreeMap::from([(PathBuf::new(), 0)]);
        let mut digest = RevisionDigest::default();
        digest.update((
            &selected,
            env!("CARGO_PKG_VERSION"),
            options.respect_ignore,
            options.max_file_bytes,
        ))?;
        timing.checkpoint("setup");
        for (path, info) in index.files() {
            let source = crate::vfs::read_to_string(path)
                .with_context(|| format!("reading {} for project view", path.display()))?;
            if index.content_hash(path) != Some(content_hash_of(&source)) {
                bail!(
                    "{} changed during indexing; retry project query.",
                    path.display()
                );
            }
            timing.checkpoint("source_read");
            digest.update((path, hash(&source)?, &info.gaps))?;
            timing.checkpoint("source_digest");
            project.lines.insert(path.clone(), LineIndex::new(&source));
            project.sources.insert(path.clone(), source);
            timing.checkpoint("source_lines");
            let relative = path.strip_prefix(&project.root)?.to_path_buf();
            let mut directory = PathBuf::new();
            let mut parent = 0;
            for component in relative
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .components()
            {
                directory.push(component);
                parent = if let Some(id) = directories.get(&directory) {
                    *id
                } else {
                    let id = project.add(Node {
                        name: component.as_os_str().to_string_lossy().into(),
                        kind: "directory".into(),
                        path: directory.clone(),
                        parent: Some(parent),
                        children: Vec::new(),
                        symbol: None,
                    });
                    directories.insert(directory.clone(), id);
                    id
                };
            }
            let file = project.add(Node {
                name: relative.file_name().unwrap().to_string_lossy().into(),
                kind: "file".into(),
                path: relative,
                parent: Some(parent),
                children: Vec::new(),
                symbol: None,
            });
            let mut symbols = info
                .symbols
                .iter()
                .filter_map(|id| index.symbol(*id))
                .collect::<Vec<_>>();
            symbols.sort_by_key(|s| (s.full_span.start, std::cmp::Reverse(s.full_span.end), s.id));
            let mut stack: Vec<(Span, usize)> = Vec::new();
            timing.checkpoint("hierarchy");
            for symbol in symbols {
                digest.update(symbol)?;
                timing.checkpoint("symbol_digest");
                while stack.last().is_some_and(|(span, _)| {
                    *span == symbol.full_span || !span.contains(symbol.full_span)
                }) {
                    stack.pop();
                }
                let parent = stack.last().map_or(file, |(_, node)| *node);
                let id = project.add(Node {
                    name: symbol.name.clone(),
                    kind: symbol.kind.as_str().into(),
                    path: project.nodes[file].path.clone(),
                    parent: Some(parent),
                    children: Vec::new(),
                    symbol: Some(symbol.id),
                });
                project.symbol_nodes.insert(symbol.id, id);
                stack.push((symbol.full_span, id));
                timing.checkpoint("hierarchy");
            }
        }
        for reference in &index.references {
            digest.update((&reference.file, reference))?;
        }
        timing.checkpoint("reference_digest");
        digest.update((
            &index.skipped,
            &scanned.skipped_symlinks,
            &scanned.unsupported,
            &project.manifests.snapshots,
        ))?;
        project.revision = digest.finish();
        timing.checkpoint("finish");
        Ok((project, timing.phases))
    }

    fn add(&mut self, node: Node) -> usize {
        let id = self.nodes.len();
        if let Some(parent) = node.parent {
            self.nodes[parent].children.push(id);
        }
        self.nodes.push(node);
        id
    }

    fn handle(&self, id: usize) -> String {
        format!("frp1:{}:{id:x}", &self.revision[..32])
    }

    fn resolve_handle(&self, handle: &str) -> Result<usize> {
        let mut parts = handle.split(':');
        if parts.next() != Some("frp1") || parts.next() != Some(&self.revision[..32]) {
            bail!("stale or invalid project handle; obtain a fresh project map.");
        }
        let id = usize::from_str_radix(parts.next().context("missing handle identity")?, 16)
            .context("invalid handle identity")?;
        if parts.next().is_some() || id >= self.nodes.len() {
            bail!("unknown project handle");
        }
        Ok(id)
    }

    fn explicit_handle(&self, id: &str, revision: Option<&str>) -> Result<String> {
        if let Some(revision) = revision {
            if revision != self.revision && revision != &self.revision[..32] {
                bail!("stale revision; obtain a fresh project map.");
            }
            Ok(format!("frp1:{}:{id}", &self.revision[..32]))
        } else {
            Ok(id.to_owned())
        }
    }

    fn target(&self, target: &str) -> Result<usize> {
        if target.starts_with("frp1:") {
            return self.resolve_handle(target);
        }
        let path = self
            .root
            .join(target)
            .canonicalize()
            .with_context(|| format!("resolving project path {target}"))?;
        let relative = path
            .strip_prefix(&self.root)
            .context("project path is outside the workspace")?;
        self.nodes
            .iter()
            .position(|n| n.symbol.is_none() && n.path == relative)
            .context(
                "path has no indexed project node; inspect project gaps or change scan options.",
            )
    }

    fn local(&self, node: usize) -> bool {
        self.nodes[node]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .is_some_and(|s| s.kind.is_local())
    }

    fn within(&self, mut node: usize, scope: usize) -> bool {
        loop {
            if node == scope {
                return true;
            }
            let Some(parent) = self.nodes[node].parent else {
                return false;
            };
            node = parent;
        }
    }

    fn source(&self, id: usize) -> Result<(&str, Span)> {
        let node = &self.nodes[id];
        let path = self.root.join(&node.path);
        let source = self
            .sources
            .get(&path)
            .context("directory nodes have no source text")?;
        let span = node
            .symbol
            .and_then(|id| self.index.symbol(id))
            .map_or(Span::new(0, source.len()), |s| s.full_span);
        if source.get(span.start..span.end).is_none() {
            bail!("indexed span is outside its source snapshot.");
        }
        Ok((source, span))
    }

    fn signature(&self, id: usize) -> Result<Value> {
        let Some(symbol) = self.nodes[id].symbol.and_then(|s| self.index.symbol(s)) else {
            return Ok(Value::Null);
        };
        let (source, span) = self.source(id)?;
        let mut parses = self.parses.borrow_mut();
        if !parses.contains_key(&symbol.file) {
            parses.insert(
                symbol.file.clone(),
                Parsers::new().parse(symbol.language, source)?,
            );
        }
        let parsed = &parses[&symbol.file];
        let mut node = parsed
            .root()
            .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end);
        while let Some(current) = node {
            if !span.contains(Span::from(current)) {
                break;
            }
            let body = current.child_by_field_name("body").or_else(|| {
                ["value", "initializer"].iter().find_map(|field| {
                    current
                        .child_by_field_name(field)
                        .and_then(|value| value.child_by_field_name("body"))
                })
            });
            if let Some(body) = body {
                if body.start_byte() >= symbol.name_span.end && body.start_byte() <= span.end {
                    let header = source
                        .get(span.start..body.start_byte())
                        .context("header crosses a UTF-8 boundary")?
                        .trim_end();
                    return Ok(
                        json!({"basis": "syntax-header", "text": bounded_text(header, 512)}),
                    );
                }
            }
            node = current.parent();
        }
        Ok(json!({"basis": "name-only", "text": bounded_text(&symbol.name, 512)}))
    }

    fn coverage(&self) -> Value {
        let mut gaps = BTreeMap::new();
        for (_, file) in self.index.files() {
            for gap in &file.gaps {
                *gaps.entry(gap.cause()).or_insert(0usize) += 1;
            }
        }
        json!({"indexed_files": self.index.file_count(), "skipped_files": self.index.skipped.len(),
            "skipped_symlinks": self.scanned.skipped_symlinks.len(), "unsupported_files": self.scanned.unsupported.values().sum::<usize>(),
            "files_by_gap": gaps, "unresolved_references": self.index.references.iter().filter(|r| r.target.is_none()).count(),
            "respect_ignore": self.options.respect_ignore, "max_file_bytes": self.options.max_file_bytes,
            "hierarchy": "directories and lexical spans", "architecture": "not inferred",
            "manifests": {"discovered": self.manifests.snapshots.len(), "parsed": self.manifests.packages.len(),
                "gaps": self.manifests.gaps.len(), "ecosystems": ["cargo", "npm"],
                "scope": "selected scan root", "resolution": "not-attempted"}})
    }

    fn envelope(&self, query: &str) -> Value {
        json!({"schema": "fr-project-1", "revision": self.revision, "handle_prefix": format!("frp1:{}:", &self.revision[..32]), "query": query, "coverage": self.coverage()})
    }

    fn map(
        &self,
        target: &str,
        depth: usize,
        locals: bool,
        limit: usize,
        cursor: Option<&str>,
        fields: &[Field],
    ) -> Result<Value> {
        if depth > 64 {
            bail!("depth must be at most 64");
        }
        check_limit(limit)?;
        if fields.is_empty() || fields.len() > 13 {
            bail!("choose between 1 and 13 map fields");
        }
        let selected = self.target(target)?;
        let mut stack = vec![(selected, None, 0)];
        let mut visible = Vec::new();
        let mut hidden_locals = 0;
        let mut hidden_depth = 0;
        while let Some((id, parent, level)) = stack.pop() {
            let hidden = !locals && self.local(id) && id != selected;
            if hidden {
                hidden_locals += 1;
            } else if level <= depth {
                visible.push((id, parent, level));
            } else {
                hidden_depth += 1;
            }
            for child in self.nodes[id].children.iter().rev() {
                stack.push((
                    *child,
                    if hidden { parent } else { Some(id) },
                    if hidden { level } else { level + 1 },
                ));
            }
        }
        let key = format!(
            "frpc1:{}",
            &hash((&self.revision, "map", selected, depth, locals, fields))?[..32]
        );
        let (start, end, page) = page(visible.len(), limit, cursor, &key)?;
        let rows = self.rows(&visible[start..end], fields)?;
        let mut result = self.envelope("map");
        result["root"] = json!(self.handle(selected));
        result["columns"] = json!(fields);
        result["rows"] = json!(rows);
        result["page"] = page;
        result["omitted"] = json!({"locals": hidden_locals, "depth": hidden_depth});
        Ok(result)
    }

    fn rows(
        &self,
        visible: &[(usize, Option<usize>, usize)],
        fields: &[Field],
    ) -> Result<Vec<Vec<Value>>> {
        let mut rows = Vec::new();
        for (id, parent, level) in visible {
            let node = &self.nodes[*id];
            let symbol = node.symbol.and_then(|s| self.index.symbol(s));
            let mut row = Vec::new();
            for field in fields {
                row.push(match field {
                    Field::Id => json!(format!("{id:x}")),
                    Field::Handle => json!(self.handle(*id)),
                    Field::Parent => json!(parent.map(|p| format!("{p:x}"))),
                    Field::Kind => json!(node.kind),
                    Field::Name => bounded_text(&node.name, 160),
                    Field::Path => bounded_text(&node.path.to_string_lossy(), 512),
                    Field::Line => self.source(*id).ok().map_or(Value::Null, |(source, span)| {
                        json!(
                            self.lines[&self.root.join(&node.path)]
                                .line_col(symbol.map_or(span.start, |s| s.name_span.start), source)
                                .line
                        )
                    }),
                    Field::Children => json!(node.children.len()),
                    Field::Depth => json!(level),
                    Field::Language => json!(self
                        .index
                        .file(&self.root.join(&node.path))
                        .map(|f| f.language.name())),
                    Field::Exported => json!(symbol.map(|s| s.exported)),
                    Field::Signature => self.signature(*id)?,
                    Field::Qualifier => symbol
                        .and_then(|s| s.qualifier.as_ref())
                        .map_or(Value::Null, |q| bounded_text(q, 160)),
                });
            }
            rows.push(row);
        }
        Ok(rows)
    }

    fn source_slice(&self, id: usize, offset: usize, bytes: usize) -> Result<Value> {
        let (source, span) = self.source(id)?;
        let text = &source[span.start..span.end];
        let length = source_slice_length(text, offset, bytes).ok_or_else(|| {
            anyhow::anyhow!("source offset must be a UTF-8 boundary within the selected node.")
        })?;
        let end = offset + length;
        Ok(
            json!({"text": &text[offset..end], "span": {"start": span.start + offset, "end": span.start + end},
            "offset": offset, "total_bytes": text.len(), "returned_bytes": end - offset, "next_offset": (end < text.len()).then_some(end)}),
        )
    }

    fn show(&self, options: &ShowOptions) -> Result<Value> {
        let ShowOptions {
            handle,
            revision,
            source: source_requested,
            offset,
            bytes,
            relations,
            limit,
            cursor,
        } = options;
        let (source_requested, offset, bytes, relations, limit, cursor) = (
            *source_requested,
            *offset,
            *bytes,
            *relations,
            *limit,
            cursor.as_deref(),
        );
        check_limit(limit)?;
        if !(4..=65536).contains(&bytes) {
            bail!("source bytes must be between 4 and 65536.");
        }
        let id = self.resolve_handle(&self.explicit_handle(handle, revision.as_deref())?)?;
        let node = &self.nodes[id];
        let mut result = self.envelope("show");
        result["node"] = json!({"handle": self.handle(id), "parent": node.parent.map(|p| self.handle(p)), "kind": node.kind,
            "name": bounded_text(&node.name, 160), "path": bounded_text(&node.path.to_string_lossy(), 512), "children": node.children.len(),
            "signature": self.signature(id)?, "qualifier": node.symbol.and_then(|s| self.index.symbol(s)).and_then(|s| s.qualifier.as_ref()).map(|q| bounded_text(q, 160))});
        if node.kind != "directory" {
            let (source, span) = self.source(id)?;
            let name = node
                .symbol
                .and_then(|s| self.index.symbol(s))
                .map_or(span, |s| s.name_span);
            result["node"]["span"] = json!(span);
            result["node"]["position"] =
                json!(self.lines[&self.root.join(&node.path)].line_col(name.start, source));
        }
        if source_requested {
            result["source"] = self.source_slice(id, offset, bytes)?;
        }
        if relations {
            result["relations"] = self.relations(id, limit, cursor)?;
        }
        Ok(result)
    }

    fn relations(&self, id: usize, limit: usize, cursor: Option<&str>) -> Result<Value> {
        let node = &self.nodes[id];
        let file = self.root.join(&node.path);
        let (_, span) = self.source(id)?;
        let mut rows = Vec::new();
        if let Some(info) = self.index.file(&file) {
            for import in &info.imports {
                rows.push(json!({"direction": "file-import", "path": bounded_text(&import.path, 256), "glob": import.is_glob,
                    "names": import.names.len(), "basis": "source-declaration"}));
            }
        }
        for reference in &self.index.references {
            let outgoing = reference.file == file && span.contains(reference.span);
            let incoming = reference
                .target
                .and_then(|s| self.index.symbol(s))
                .is_some_and(|s| {
                    if let Some(symbol) = node.symbol {
                        s.id == symbol
                    } else {
                        s.file == file
                    }
                });
            for (direction, include) in [("outgoing", outgoing), ("incoming", incoming)] {
                if !include {
                    continue;
                }
                let target = reference
                    .target
                    .and_then(|s| self.symbol_nodes.get(&s))
                    .map(|n| self.handle(*n));
                let source = self
                    .sources
                    .get(&reference.file)
                    .context("reference file is outside the source snapshot.")?;
                rows.push(json!({"direction": direction, "kind": reference.kind, "name": bounded_text(&reference.name, 160),
                    "path": bounded_text(&reference.file.strip_prefix(&self.root)?.to_string_lossy(), 512),
                    "line": self.lines[&reference.file].line_col(reference.span.start, source).line, "target": target, "confidence": reference.confidence}));
            }
        }
        let key = format!("frpc1:{}", &hash((&self.revision, "relations", id))?[..32]);
        let (start, end, page) = page(rows.len(), limit, cursor, &key)?;
        Ok(
            json!({"items": &rows[start..end], "page": page, "scope": "file imports and indexed references; nested source spans included."}),
        )
    }

    fn gaps(&self, limit: usize, cursor: Option<&str>) -> Result<Value> {
        let mut rows = self.manifests.gaps.clone();
        for (path, reason) in self
            .index
            .skipped
            .iter()
            .chain(&self.scanned.skipped_symlinks)
        {
            rows.push(json!({"path": bounded_text(&path.strip_prefix(&self.root)?.to_string_lossy(), 512), "reason": bounded_text(reason, 512)}));
        }
        for (path, file) in self.index.files() {
            for gap in &file.gaps {
                rows.push(json!({"path": bounded_text(&path.strip_prefix(&self.root)?.to_string_lossy(), 512), "reason": gap.cause()}));
            }
        }
        for (extension, count) in &self.scanned.unsupported {
            rows.push(json!({"extension": bounded_text(extension, 160), "count": count, "reason": "unsupported extension"}));
        }
        let key = format!("frpc1:{}", &hash((&self.revision, "gaps"))?[..32]);
        let (start, end, page) = page(rows.len(), limit, cursor, &key)?;
        let mut result = self.envelope("gaps");
        result["items"] = json!(&rows[start..end]);
        result["page"] = page;
        Ok(result)
    }

    fn packages(&self, limit: usize, cursor: Option<&str>) -> Result<Value> {
        let key = format!("frpc1:{}", &hash((&self.revision, "packages"))?[..32]);
        let (start, end, page) = page(self.manifests.packages.len(), limit, cursor, &key)?;
        let mut result = self.envelope("packages");
        result["items"] = json!(&self.manifests.packages[start..end]);
        result["page"] = page;
        result["scope"] =
            json!("Manifest roots; source ownership and workspace membership are not inferred.");
        Ok(result)
    }

    fn selected_manifest(&self, manifest: Option<&Path>) -> Result<Option<PathBuf>> {
        manifest
            .map(|path| -> Result<PathBuf> {
                let path = self
                    .root
                    .join(path)
                    .canonicalize()
                    .context("manifest path cannot be resolved")?;
                let relative = path
                    .strip_prefix(&self.root)
                    .context("manifest is outside the project root")?;
                if !self.manifests.snapshots.contains_key(&path) {
                    bail!("manifest was not discovered; check the selected root and scan options.");
                }
                Ok(relative.to_path_buf())
            })
            .transpose()
    }

    fn dependencies(
        &self,
        manifest: Option<&Path>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Value> {
        let selected = self.selected_manifest(manifest)?;
        let rows: Vec<_> = self
            .manifests
            .declarations
            .iter()
            .filter(|(path, _)| selected.as_ref().is_none_or(|selected| path == selected))
            .map(|(_, row)| row)
            .collect();
        let key = format!(
            "frpc1:{}",
            &hash((&self.revision, "dependencies", &selected))?[..32]
        );
        let (start, end, page) = page(rows.len(), limit, cursor, &key)?;
        let mut result = self.envelope("dependencies");
        result["items"] = json!(&rows[start..end]);
        result["page"] = page;
        result["scope"] = json!("Declared constraints and patterns; no lockfile resolution, pattern expansion or inheritance.");
        Ok(result)
    }

    fn links(&self, manifest: Option<&Path>, limit: usize, cursor: Option<&str>) -> Result<Value> {
        check_limit(limit)?;
        let selected = self.selected_manifest(manifest)?;
        let rows = links::collect(&self.manifests, &self.root, selected.as_deref());
        let key = format!(
            "frpc1:{}",
            &hash((&self.revision, "links", &selected))?[..32]
        );
        let (start, end, page) = page(rows.len(), limit, cursor, &key)?;
        let mut result = self.envelope("links");
        result["items"] = json!(&rows[start..end]);
        result["page"] = page;
        result["scope"] = json!("Observed local manifests and workspace pattern candidates; package-manager resolution remains unchecked.");
        Ok(result)
    }

    fn workspaces(
        &self,
        manifest: Option<&Path>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Value> {
        check_limit(limit)?;
        let selected = self.selected_manifest(manifest)?;
        let rows = links::ownership(&self.manifests, &self.root, selected.as_deref());
        let key = format!(
            "frpc1:{}",
            &hash((&self.revision, "workspaces", &selected))?[..32]
        );
        let (start, end, page) = page(rows.len(), limit, cursor, &key)?;
        let mut result = self.envelope("workspaces");
        result["items"] = json!(&rows[start..end]);
        result["page"] = page;
        result["scope"] = json!("Cargo ownership and membership from observed manifests; package-manager validation remains unchecked.");
        Ok(result)
    }

    pub fn report(&self, command: &Command) -> Result<Value> {
        match command {
            Command::Find(options) => self.find(options),
            Command::Select(options) => self.select(options),
            Command::Map {
                target,
                revision,
                depth,
                locals,
                limit,
                cursor,
                fields,
            } => self.map(
                &self.explicit_handle(target, revision.as_deref())?,
                *depth,
                *locals,
                *limit,
                cursor.as_deref(),
                fields,
            ),
            Command::Show(options) => self.show(options),
            Command::Calls {
                selection,
                direction,
            } => self.calls(selection, *direction),
            Command::Implementations(selection) => self.implementations(selection),
            Command::Routes(selection) => self.routes(selection, false, false),
            Command::Contracts { selection, types } => self.routes(selection, true, *types),
            Command::Features(options) => self.features(options),
            Command::Schemas(selection) => self.schemas(selection),
            Command::Configuration(selection) => self.configuration(selection),
            Command::Tests { selection, depth } => self.tests(selection, *depth),
            Command::Packages { limit, cursor } => self.packages(*limit, cursor.as_deref()),
            Command::Dependencies {
                manifest,
                limit,
                cursor,
            } => self.dependencies(manifest.as_deref(), *limit, cursor.as_deref()),
            Command::Links {
                manifest,
                limit,
                cursor,
            } => self.links(manifest.as_deref(), *limit, cursor.as_deref()),
            Command::Workspaces {
                manifest,
                limit,
                cursor,
            } => self.workspaces(manifest.as_deref(), *limit, cursor.as_deref()),
            Command::Gaps { limit, cursor } => self.gaps(*limit, cursor.as_deref()),
        }
    }

    pub fn verify(&self, selected: &Path) -> Result<()> {
        if manifests::discover(selected, self.options)? != self.manifests.snapshots {
            bail!("manifest inventory or content changed during the project query; retry.");
        }
        for (path, source) in &self.sources {
            if crate::vfs::read_to_string(path).as_ref().ok() != Some(source) {
                bail!(
                    "{} changed during the project query; retry.",
                    path.display()
                );
            }
        }
        let current = scan(selected, self.options)?;
        if current.files != self.scanned.files
            || current.skipped_too_large != self.scanned.skipped_too_large
            || current.skipped_symlinks != self.scanned.skipped_symlinks
            || current.unsupported != self.scanned.unsupported
        {
            bail!("workspace inventory changed during the project query; retry.");
        }
        Ok(())
    }
}
