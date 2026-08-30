//! Entry-point detection driven by declarative catalogs.

use crate::index::Index;
use crate::lang::Language;
use crate::model::{ReferenceKind, Symbol, SymbolId, SymbolKind};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// What kind of entry point this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// Program entry: `fn main`, `if __name__ == "__main__"`.
    CliMain,
    /// A CLI subcommand handler.
    CliSubcommand,
    /// HTTP route handler.
    HttpRoute,
    /// WebSocket handler.
    Websocket,
    /// Message queue or stream consumer.
    QueueConsumer,
    /// Cron or scheduled job.
    ScheduledJob,
    /// Serverless function handler.
    ServerlessHandler,
    /// Test function.
    Test,
    /// Public API of a library.
    ExportedApi,
    /// Externally settable configuration input (Terraform variable, Helm value).
    InfraInput,
    /// Declared network exposure (Service, Ingress, open security group).
    InfraExposure,
    /// A documentation entry point (README and friends).
    Doc,
}

impl EntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::CliMain => "cli-main",
            EntryKind::CliSubcommand => "cli-subcommand",
            EntryKind::HttpRoute => "http-route",
            EntryKind::Websocket => "websocket",
            EntryKind::QueueConsumer => "queue-consumer",
            EntryKind::ScheduledJob => "scheduled-job",
            EntryKind::ServerlessHandler => "serverless-handler",
            EntryKind::Test => "test",
            EntryKind::ExportedApi => "exported-api",
            EntryKind::InfraInput => "infra-input",
            EntryKind::InfraExposure => "infra-exposure",
            EntryKind::Doc => "doc",
        }
    }
}

/// Whether an entry point is reachable by a remote attacker or only locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreatModel {
    Remote,
    Local,
    None,
}

/// A language a rule applies to, or every language the tool knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliesTo {
    /// `*`, every language.
    Any,
    Language(Language),
}

impl AppliesTo {
    pub fn covers(self, language: Language) -> bool {
        match self {
            AppliesTo::Any => true,
            AppliesTo::Language(one) => one == language,
        }
    }
}

impl<'de> Deserialize<'de> for AppliesTo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let written = String::deserialize(deserializer)?;
        if written == "*" {
            return Ok(AppliesTo::Any);
        }
        Language::from_name(&written)
            .map(AppliesTo::Language)
            .ok_or_else(|| {
                let known: Vec<&str> = Language::ALL.iter().map(|l| l.name()).collect();
                serde::de::Error::custom(format!(
                    "unknown language `{written}`; expected `*` or one of {}",
                    known.join(", ")
                ))
            })
    }
}

/// One rule from a catalog.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Human-readable rule name.
    pub id: String,
    pub kind: EntryKind,
    #[serde(default = "default_threat")]
    pub threat_model: ThreatModel,
    /// Languages this rule applies to.
    pub languages: Vec<AppliesTo>,
    #[serde(default)]
    pub matches: Matcher,
}

fn default_threat() -> ThreatModel {
    ThreatModel::None
}

/// Conditions a symbol must meet.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matcher {
    /// The file name must start with this.
    #[serde(default)]
    pub file_prefix: Option<String>,
    /// Exact symbol name.
    #[serde(default)]
    pub name: Option<String>,
    /// Symbol name prefix.
    #[serde(default)]
    pub name_prefix: Option<String>,
    /// Symbol name suffix.
    #[serde(default)]
    pub name_suffix: Option<String>,
    /// Symbol kind, e.g.
    #[serde(default)]
    pub symbol_kind: Option<SymbolKind>,
    /// The file name must equal this.
    #[serde(default)]
    pub file_name: Option<String>,
    /// The file path must contain this segment.
    #[serde(default)]
    pub path_contains: Option<String>,
    /// The file name must end with this.
    #[serde(default)]
    pub file_suffix: Option<String>,
    /// The symbol must be exported.
    #[serde(default)]
    pub exported: Option<bool>,
    /// The symbol must be at file top level (no enclosing symbol).
    #[serde(default)]
    pub top_level: Option<bool>,
    /// An annotation written immediately above the symbol, without its punctuation: `test`
    /// matches Rust's `#[test]` and `#[tokio::test]`, Python's `@pytest.mark` and Java's
    /// `@Test`.
    #[serde(default)]
    pub annotated_with: Option<String>,
    /// The module's `if __name__ == "__main__":` block calls this symbol.
    #[serde(default)]
    pub called_from_main_guard: Option<bool>,
    /// A directive at the top of the file, or at the top of the symbol's own body.
    #[serde(default)]
    pub file_directive: Option<String>,
    /// The declaration begins with this keyword.
    #[serde(default)]
    pub declaration_keyword: Option<String>,
    /// The annotation's first string argument must start with this.
    #[serde(default)]
    pub annotation_argument_prefix: Option<String>,
}

impl Matcher {
    /// Does this matcher say anything at all?
    pub fn names_a_condition(&self) -> bool {
        let Matcher {
            file_prefix,
            name,
            name_prefix,
            name_suffix,
            symbol_kind,
            file_name,
            path_contains,
            file_suffix,
            exported,
            top_level,
            annotated_with,
            called_from_main_guard,
            file_directive,
            declaration_keyword,
            annotation_argument_prefix,
        } = self;
        file_prefix.is_some()
            || name.is_some()
            || name_prefix.is_some()
            || name_suffix.is_some()
            || symbol_kind.is_some()
            || file_name.is_some()
            || path_contains.is_some()
            || file_suffix.is_some()
            || exported.is_some()
            || top_level.is_some()
            || annotated_with.is_some()
            || called_from_main_guard.is_some()
            || file_directive.is_some()
            || declaration_keyword.is_some()
            // Not on its own: it qualifies `annotated_with`, and `check_rules` rejects
            // it without one.
            || (annotation_argument_prefix.is_some() && annotated_with.is_some())
    }
}

/// A detected entry point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrypoint {
    pub symbol: SymbolId,
    pub kind: EntryKind,
    pub threat_model: ThreatModel,
    /// The catalog rule that matched, so a surprising result leads back to it.
    pub rule: String,
}

/// A loaded set of rules.
#[derive(Debug, Default)]
pub struct Catalog {
    pub rules: Vec<Rule>,
}

/// The built-in catalogs, embedded so the tool works with no external files.
const BUILTIN: &[(&str, &str)] = &[
    ("rust", include_str!("../../catalogs/rust.yaml")),
    ("go", include_str!("../../catalogs/go.yaml")),
    ("python", include_str!("../../catalogs/python.yaml")),
    ("typescript", include_str!("../../catalogs/typescript.yaml")),
    ("zig", include_str!("../../catalogs/zig.yaml")),
    ("java", include_str!("../../catalogs/java.yaml")),
    ("bash", include_str!("../../catalogs/bash.yaml")),
    ("infra", include_str!("../../catalogs/infra.yaml")),
    ("docs", include_str!("../../catalogs/docs.yaml")),
    ("markup", include_str!("../../catalogs/markup.yaml")),
];

/// Reject a rule that cannot mean what it says.
fn check_rules(rules: &[Rule]) -> Result<()> {
    for rule in rules {
        // Most specific first: a rule with only `annotation_argument_prefix` also names
        // no condition, and being told which field is wrong beats being told the rule is.
        if rule.matches.annotation_argument_prefix.is_some()
            && rule.matches.annotated_with.is_none()
        {
            anyhow::bail!(
                "rule '{}' asks for an annotation argument without naming the annotation",
                rule.id
            );
        }
        if !rule.matches.names_a_condition() {
            anyhow::bail!(
                "rule '{}' names no condition, so it would match nothing at all",
                rule.id
            );
        }
    }
    Ok(())
}

impl Catalog {
    /// Load the built-in catalogs.
    pub fn builtin() -> Result<Self> {
        let mut rules = Vec::new();
        for (name, yaml) in BUILTIN {
            let parsed: Vec<Rule> = serde_yaml::from_str(yaml)
                .with_context(|| format!("parsing built-in catalog '{name}'"))?;
            check_rules(&parsed).with_context(|| format!("built-in catalog '{name}'"))?;
            rules.extend(parsed);
        }
        Ok(Catalog { rules })
    }

    /// Load additional rules from a directory of YAML files.
    pub fn load_dir(&mut self, dir: &Path) -> Result<usize> {
        let mut added = 0;
        for path in crate::vfs::read_dir(dir)
            .with_context(|| format!("reading catalog directory {}", dir.display()))?
        {
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let text = crate::vfs::read_to_string(&path)?;
            let parsed: Vec<Rule> = serde_yaml::from_str(&text)
                .with_context(|| format!("parsing catalog {}", path.display()))?;
            check_rules(&parsed).with_context(|| format!("catalog {}", path.display()))?;
            added += parsed.len();
            self.rules.extend(parsed);
        }
        Ok(added)
    }

    /// Find every entry point in an index.
    pub fn detect(&self, index: &Index) -> Vec<Entrypoint> {
        let mut found = Vec::new();
        // One read per symbol instead of one per rule.
        let mut cached: Option<(std::path::PathBuf, String)> = None;
        for symbol in &index.symbols {
            if cached.as_ref().is_none_or(|(path, _)| path != &symbol.file) {
                cached = crate::vfs::read_to_string(&symbol.file)
                    .ok()
                    .map(|text| (symbol.file.clone(), text));
            }
            let source = cached.as_ref().map(|(_, text)| text.as_str());
            for rule in &self.rules {
                if rule_applies(rule, symbol, source) {
                    found.push(Entrypoint {
                        symbol: symbol.id,
                        kind: rule.kind,
                        threat_model: rule.threat_model,
                        rule: rule.id.clone(),
                    });
                }
            }
        }
        found.extend(python_packaging_entrypoints(index));
        found.sort_by_key(|e| (e.kind, e.symbol));
        found.dedup_by_key(|e| (e.symbol, e.kind));
        found
    }
}

/// Entry points Python packaging declares, which no catalog rule can express.
fn python_packaging_entrypoints(index: &Index) -> Vec<Entrypoint> {
    let mut found = dunder_main_entrypoints(index);
    found.extend(console_script_entrypoints(index));
    found
}

/// Functions a package's `__main__.py` calls at module level.
fn dunder_main_entrypoints(index: &Index) -> Vec<Entrypoint> {
    let mut found = Vec::new();
    for (path, info) in index.files() {
        if info.language != Language::Python {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) != Some("__main__.py") {
            continue;
        }
        let callables: Vec<&Symbol> = info
            .symbols
            .iter()
            .filter_map(|id| index.symbol(*id))
            .filter(|s| s.kind.is_callable())
            .collect();
        for reference in info.references.iter().map(|i| &index.references[*i]) {
            if reference.kind != ReferenceKind::Call {
                continue;
            }
            if callables
                .iter()
                .any(|c| c.full_span.contains(reference.span))
            {
                continue;
            }
            let Some(target) = reference.target.and_then(|t| index.symbol(t)) else {
                continue;
            };
            if !target.kind.is_callable() {
                continue;
            }
            found.push(Entrypoint {
                symbol: target.id,
                kind: EntryKind::CliMain,
                threat_model: ThreatModel::Local,
                rule: format!(
                    "python-package-main (called at module level of {})",
                    path.display()
                ),
            });
        }
    }
    found
}

/// Functions that setup.py `console_scripts` or pyproject.toml `[project.scripts]` declare as
/// installed commands.
fn console_script_entrypoints(index: &Index) -> Vec<Entrypoint> {
    let mut found = Vec::new();
    for (file, basis) in packaging_files(index) {
        let Ok(text) = crate::vfs::read_to_string(&file) else {
            continue;
        };
        for (script, module, function) in script_targets(&text, &basis) {
            let Some(symbol) = module_function(index, &module, &function) else {
                continue;
            };
            found.push(Entrypoint {
                symbol,
                kind: EntryKind::CliMain,
                threat_model: ThreatModel::Local,
                rule: format!(
                    "python-console-script ({} declares {} = {}:{}, matched line by line)",
                    basis, script, module, function
                ),
            });
        }
    }
    found
}

/// The packaging files near this workspace's code, each with the label a detection cites as its
/// basis.
fn packaging_files(index: &Index) -> Vec<(std::path::PathBuf, String)> {
    let mut out = Vec::new();
    let mut root: Option<std::path::PathBuf> = None;
    for (path, _) in index.files() {
        if path.file_name().and_then(|n| n.to_str()) == Some("setup.py") {
            out.push((path.clone(), "setup.py console_scripts".to_string()));
        }
        root = match root {
            None => path.parent().map(Path::to_path_buf),
            Some(mut shared) => {
                while !path.starts_with(&shared) {
                    let Some(up) = shared.parent() else { break };
                    shared = up.to_path_buf();
                }
                Some(shared)
            }
        };
    }
    let Some(root) = root else { return out };
    let mut dirs = std::collections::BTreeSet::new();
    // One level above the deepest shared directory as well.
    if let Some(above) = root.parent() {
        dirs.insert(above.to_path_buf());
    }
    for (path, _) in index.files() {
        let mut dir = path.parent();
        while let Some(d) = dir {
            dirs.insert(d.to_path_buf());
            if d == root {
                break;
            }
            dir = d.parent();
        }
    }
    for dir in dirs {
        let candidate = dir.join("pyproject.toml");
        if crate::vfs::exists(&candidate) {
            out.push((candidate, "pyproject.toml [project.scripts]".to_string()));
        }
    }
    out
}

/// The `(script, module, function)` entries a packaging file declares.
fn script_targets(text: &str, basis: &str) -> Vec<(String, String, String)> {
    let toml = basis.starts_with("pyproject");
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !inside {
            inside = match toml {
                true => trimmed == "[project.scripts]",
                false => trimmed.contains("console_scripts"),
            };
            continue;
        }
        let closed = match toml {
            true => trimmed.starts_with('['),
            false => trimmed.starts_with(']') || trimmed.contains(']'),
        };
        if closed {
            inside = false;
            continue;
        }
        let entry = trimmed
            .trim_end_matches(',')
            .trim_matches(['"', '\''])
            .replace(['"', '\''], "");
        let Some((script, target)) = entry.split_once('=') else {
            continue;
        };
        let Some((module, function)) = target.split_once(':') else {
            continue;
        };
        out.push((
            script.trim().to_string(),
            module.trim().to_string(),
            function.trim().to_string(),
        ));
    }
    out
}

/// The top-level function `module:function` names, when the workspace holds it.
fn module_function(index: &Index, module: &str, function: &str) -> Option<SymbolId> {
    let relative: std::path::PathBuf = module.split('.').collect();
    let suffixes = [relative.with_extension("py"), relative.join("__init__.py")];
    for (path, info) in index.files() {
        if !suffixes.iter().any(|suffix| path.ends_with(suffix)) {
            continue;
        }
        let hit = info
            .symbols
            .iter()
            .filter_map(|id| index.symbol(*id))
            .find(|s| {
                s.name == function && s.kind == SymbolKind::Function && s.container.is_none()
            });
        if let Some(symbol) = hit {
            return Some(symbol.id);
        }
    }
    None
}

/// The roots a reachability question starts from.
#[derive(Debug, Clone, Default)]
pub struct Entrypoints(Vec<SymbolId>);

impl Entrypoints {
    /// Everything the built-in catalogs recognise as an entry point: a `main`, an
    /// HTTP handler, a `#[test]`, a Kubernetes probe path.
    pub fn detect(index: &Index) -> anyhow::Result<Self> {
        Ok(Self::from_catalog(&Catalog::builtin()?, index))
    }

    /// The same, from a catalog the caller has already extended.
    pub fn from_catalog(catalog: &Catalog, index: &Index) -> Self {
        crate::capabilities::record_workspace(crate::capabilities::Capability::EntryPoints, index);
        Entrypoints(catalog.detect(index).iter().map(|e| e.symbol).collect())
    }

    /// Exactly these symbols and no others.
    pub fn exactly(roots: &[SymbolId]) -> Self {
        Entrypoints(roots.to_vec())
    }

    /// No roots at all: only exported symbols anchor reachability.
    pub fn none() -> Self {
        Entrypoints(Vec::new())
    }

    pub fn as_slice(&self) -> &[SymbolId] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn rule_applies(rule: &Rule, symbol: &Symbol, source: Option<&str>) -> bool {
    if !rule.languages.iter().any(|l| l.covers(symbol.language)) {
        return false;
    }

    let m = &rule.matches;
    // A matcher with no conditions would tag every symbol in the language.
    if !m.names_a_condition() {
        return false;
    }
    let file_name = symbol
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    if let Some(name) = &m.name {
        if &symbol.name != name {
            return false;
        }
    }
    if let Some(prefix) = &m.name_prefix {
        if !symbol.name.starts_with(prefix.as_str()) {
            return false;
        }
    }
    if let Some(suffix) = &m.name_suffix {
        if !symbol.name.ends_with(suffix.as_str()) {
            return false;
        }
    }
    if let Some(kind) = m.symbol_kind {
        if symbol.kind != kind {
            return false;
        }
    }
    if let Some(expected) = &m.file_name {
        if !file_name.eq_ignore_ascii_case(expected) {
            return false;
        }
    }
    if let Some(needle) = &m.path_contains {
        if !symbol.file.to_string_lossy().contains(needle.as_str()) {
            return false;
        }
    }
    if let Some(suffix) = &m.file_suffix {
        if !file_name.ends_with(suffix.as_str()) {
            return false;
        }
    }
    if let Some(prefix) = &m.file_prefix {
        if !file_name.starts_with(prefix.as_str()) {
            return false;
        }
    }
    if let Some(annotation) = &m.annotated_with {
        let Some(written) = source.and_then(|text| annotation_in(text, symbol, annotation)) else {
            return false;
        };
        if let Some(prefix) = &m.annotation_argument_prefix {
            if !annotation_argument_starts_with(&written, prefix) {
                return false;
            }
        }
    }
    if let Some(wanted) = m.called_from_main_guard {
        if source.is_some_and(|text| called_from_main_guard(text, symbol)) != wanted {
            return false;
        }
    }
    if let Some(keyword) = &m.declaration_keyword {
        if !source.is_some_and(|text| declaration_begins_with(text, symbol, keyword)) {
            return false;
        }
    }
    if let Some(directive) = &m.file_directive {
        if !source.is_some_and(|text| under_directive(text, symbol, directive)) {
            return false;
        }
    }
    if let Some(exported) = m.exported {
        if symbol.exported != exported {
            return false;
        }
    }
    if let Some(top_level) = m.top_level {
        if symbol.container.is_some() == top_level {
            return false;
        }
    }
    true
}

/// Summarise detections by kind.
pub fn summarise(entries: &[Entrypoint]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for e in entries {
        *counts.entry(e.kind.as_str()).or_default() += 1;
    }
    counts
}

/// Does the catalog have any rule that could fire for this language?
pub fn annotated_with(symbol: &Symbol, name: &str) -> bool {
    annotation_on(symbol, name).is_some()
}

/// The annotation's own text, so a caller can ask about its arguments as well as its name.
pub fn annotation_on(symbol: &Symbol, name: &str) -> Option<String> {
    annotation_in(
        &crate::vfs::read_to_string(&symbol.file).ok()?,
        symbol,
        name,
    )
}

/// [`annotation_on`] against source already in hand, so a caller checking many rules
/// against one symbol reads its file once.
fn annotation_in(source: &str, symbol: &Symbol, name: &str) -> Option<String> {
    // Inside the declaration: everything from its start up to the name it declares.
    let start = symbol.full_span.start.min(source.len());
    let up_to_name = symbol.name_span.start.clamp(start, source.len());
    if let Some(piece) = source[start..up_to_name]
        .split('@')
        .skip(1)
        .find(|piece| names_the_annotation(piece, name))
    {
        return Some(piece.to_string());
    }

    // What sits before the symbol on its own line is part of the declaration, not a line before
    // it.
    let before = &source[..start];
    let partial = before.rsplit_once('\n').map_or(before, |(_, tail)| tail);
    if partial.contains(['{', ';', '}', '(']) {
        return None;
    }
    let before = &before[..before.len() - partial.len()];

    // Above it: the unbroken run of annotation and comment lines before it.
    for line in before.lines().rev() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with('#')
            || line.starts_with('@')
        {
            let bare = line.trim_start_matches(['#', '[', '@']);
            if names_the_annotation(bare, name) {
                return Some(bare.to_string());
            }
            continue;
        }
        // Anything else ends the run of annotations.
        break;
    }
    None
}

/// Does this annotation's first string argument start with `prefix`?
fn annotation_argument_starts_with(annotation: &str, prefix: &str) -> bool {
    let Some((_, args)) = annotation.split_once('(') else {
        return false;
    };
    args.trim_start()
        .strip_prefix(['"', '\''])
        .is_some_and(|literal| literal.starts_with(prefix))
}

/// Does the symbol's own declaration open with this keyword?
fn declaration_begins_with(source: &str, symbol: &Symbol, keyword: &str) -> bool {
    let start = symbol.full_span.start.min(source.len());
    let Some(rest) = source[start..].strip_prefix(keyword) else {
        return false;
    };
    !rest
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Is this symbol called from its module's `if __name__ == "__main__":` block?
fn called_from_main_guard(source: &str, symbol: &Symbol) -> bool {
    if symbol.language != Language::Python || symbol.container.is_some() {
        return false;
    }
    // Cheap first: no guard in the file means no parse.
    if !source.contains("__main__") {
        return false;
    }
    let Ok(parsed) = crate::parse::Parsers::new().parse(Language::Python, source) else {
        return false;
    };

    let mut called = false;
    let mut cursor = parsed.root().walk();
    for statement in parsed.root().named_children(&mut cursor) {
        if statement.kind() != "if_statement" {
            continue;
        }
        let Some(condition) = statement.child_by_field_name("condition") else {
            continue;
        };
        let condition = &source[condition.byte_range()];
        if !(condition.contains("__name__") && condition.contains("__main__")) {
            continue;
        }
        let Some(body) = statement.child_by_field_name("consequence") else {
            continue;
        };
        called |= calls_by_name(body, source).contains(&symbol.name);
    }
    called
}

/// The names called anywhere under a node: `cli()` and `app.run()` name `cli` and
/// `run`.
fn calls_by_name(node: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind() == "call" {
            if let Some(function) = node.child_by_field_name("function") {
                let named = match function.kind() {
                    "attribute" => function.child_by_field_name("attribute"),
                    _ => Some(function),
                };
                if let Some(named) = named {
                    names.push(source[named.byte_range()].to_string());
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    names
}

/// Is this symbol under a directive, at the top of its file, or of its own body?
fn under_directive(source: &str, symbol: &Symbol, directive: &str) -> bool {
    let quoted = |line: &str| {
        let trimmed = line.trim().trim_end_matches(';').trim();
        trimmed == format!("\"{directive}\"") || trimmed == format!("'{directive}'")
    };
    // At the top of the file, past any leading comments and blank lines.
    let leading = source
        .lines()
        .find(|line| {
            let t = line.trim();
            !t.is_empty() && !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
        })
        .unwrap_or_default();
    if quoted(leading) {
        return true;
    }
    // Or at the top of this symbol's own body.
    let start = symbol.full_span.start.min(source.len());
    let end = symbol.full_span.end.min(source.len());
    source[start..end]
        .lines()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .is_some_and(quoted)
}

/// Does this fragment name the annotation, whatever punctuation surrounds it?
fn names_the_annotation(fragment: &str, name: &str) -> bool {
    // Whitespace ends the name as surely as a bracket does.
    fragment
        .trim()
        .trim_start_matches(['#', '[', '@'])
        .split(|c: char| c.is_whitespace() || c == '(' || c == '<')
        .next()
        .unwrap_or_default()
        .trim_end_matches([']', ')'])
        .rsplit(['.', ':'])
        .next()
        .unwrap_or_default()
        == name
}

pub fn has_rules_for(catalog: &Catalog, language: Language) -> bool {
    catalog
        .rules
        .iter()
        .any(|r| r.languages.iter().any(|l| l.covers(language)))
}

/// Languages with no entry-point rules at all, so gaps in coverage are visible.
pub fn languages_without_rules(catalog: &Catalog) -> Vec<&'static str> {
    Language::ALL
        .iter()
        .filter(|lang| {
            !catalog
                .rules
                .iter()
                .any(|r| r.languages.iter().any(|l| l.covers(**lang)))
        })
        .map(|lang| lang.name())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{ScanResult, SourceFile};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        let mut scanned = ScanResult::default();
        for (name, content) in files {
            let path = tmp.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            crate::vfs::write(&path, content).unwrap();
            // A file with no detected language stays on disk without joining the
            // index, the footing pyproject.toml has in a real scan.
            if let Some(language) = crate::lang::detect(&path) {
                scanned.files.push(SourceFile { language, path });
            }
        }
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn kinds_for(index: &Index, name: &str) -> Vec<EntryKind> {
        let catalog = Catalog::builtin().unwrap();
        let entries = catalog.detect(index);
        let target = index.find_symbols(name, None);
        assert!(!target.is_empty(), "no symbol named {name}");
        let ids: Vec<SymbolId> = target.iter().map(|s| s.id).collect();
        entries
            .iter()
            .filter(|e| ids.contains(&e.symbol))
            .map(|e| e.kind)
            .collect()
    }

    #[test]
    fn builtin_catalogs_parse() {
        let catalog = Catalog::builtin().unwrap();
        assert!(
            catalog.rules.len() >= 20,
            "expected a meaningful rule set, got {}",
            catalog.rules.len()
        );
        // Every rule must have at least one language and a usable condition.
        for rule in &catalog.rules {
            assert!(
                !rule.languages.is_empty(),
                "rule {} has no language",
                rule.id
            );
            assert!(!rule.id.is_empty());
        }
    }

    #[test]
    fn detects_rust_main_and_tests() {
        let (_tmp, index) = workspace(&[("a.rs", "fn main() {}\nfn test_thing() {}\n")]);
        assert!(kinds_for(&index, "main").contains(&EntryKind::CliMain));
        assert!(kinds_for(&index, "test_thing").contains(&EntryKind::Test));
    }

    #[test]
    fn detects_go_main_and_tests() {
        let (_tmp, index) = workspace(&[
            ("main.go", "package main\n\nfunc main() {}\n"),
            ("thing_test.go", "package main\n\nfunc TestThing() {}\n"),
        ]);
        assert!(kinds_for(&index, "main").contains(&EntryKind::CliMain));
        assert!(kinds_for(&index, "TestThing").contains(&EntryKind::Test));
    }

    #[test]
    fn detects_python_main_and_tests() {
        let (_tmp, index) = workspace(&[(
            "app.py",
            "def main():\n    pass\n\ndef test_it():\n    pass\n",
        )]);
        assert!(kinds_for(&index, "main").contains(&EntryKind::CliMain));
        assert!(kinds_for(&index, "test_it").contains(&EntryKind::Test));
    }

    const MODULE_TF: &str = "variable \"region\" {\n  type = string\n}\n\n\
                             locals {\n  prefix = \"web\"\n  spare = \"nobody\"\n}\n\n\
                             resource \"aws_instance\" \"web\" {\n  \
                             tags = { Name = local.prefix }\n}\n\n\
                             output \"web_ids\" {\n  value = aws_instance.web[*].id\n}\n";

    #[test]
    fn a_terraform_local_is_not_an_input_and_an_output_is_the_surface() {
        // Both halves reach the index as a symbol of kind `variable`.
        let (_tmp, index) = workspace(&[("main.tf", MODULE_TF)]);
        assert!(
            kinds_for(&index, "region").contains(&EntryKind::InfraInput),
            "a root-module variable is settable from outside."
        );
        assert!(
            kinds_for(&index, "prefix").is_empty(),
            "a local is the module's own working store."
        );
        assert!(
            kinds_for(&index, "web_ids").contains(&EntryKind::ExportedApi),
            "an output is what a caller reads, wherever it sits"
        );
    }

    #[test]
    fn a_local_nothing_reads_reaches_the_unused_report() {
        // Every HCL symbol counted as an entry point, so reachability started
        // everywhere and `fr unused` was structurally empty for Terraform.
        let (_tmp, index) = workspace(&[("main.tf", MODULE_TF)]);
        let catalog = Catalog::builtin().unwrap();
        let entrypoints = Entrypoints::from_catalog(&catalog, &index);
        let dead: Vec<&str> = crate::refactor::delete::find_unused(&index, &entrypoints)
            .into_iter()
            .filter_map(|id| index.symbol(id))
            .map(|s| s.name.as_str())
            .collect();
        assert!(dead.contains(&"spare"), "a local nobody reads: {dead:?}");
        assert!(
            !dead.contains(&"prefix"),
            "`local.prefix` is read: {dead:?}"
        );
        assert!(!dead.contains(&"region"), "an input is a start: {dead:?}");
    }

    #[test]
    fn a_charts_metadata_is_not_something_an_install_can_set() {
        let (_tmp, index) = workspace(&[
            ("Chart.yaml", "apiVersion: v2\nname: app\nversion: 0.1.0\n"),
            ("values.yaml", "replicaCount: 1\n"),
            (
                "templates/deployment.yaml",
                "kind: Deployment\nspec:\n  replicas: {{ .Values.replicaCount }}\n",
            ),
        ]);
        assert!(
            kinds_for(&index, "apiVersion").contains(&EntryKind::ExportedApi),
            "`--set apiVersion=v2` sets a value no template reads"
        );
        assert!(
            kinds_for(&index, "replicaCount").contains(&EntryKind::InfraInput),
            "a values key is what an install overrides"
        );
    }

    #[test]
    fn an_empty_matcher_never_matches_everything() {
        // A rule with no conditions would tag every symbol; that is always a catalog authoring
        // mistake.
        let rule = Rule {
            id: "bad".into(),
            kind: EntryKind::CliMain,
            threat_model: ThreatModel::None,
            languages: vec![AppliesTo::Language(Language::Rust)],
            matches: Matcher::default(),
        };
        let (_tmp, index) = workspace(&[("a.rs", "fn whatever() {}\n")]);
        let symbol = &index.symbols[0];
        assert!(!rule_applies(&rule, symbol, Some("fn whatever() {}\n")));
    }

    #[test]
    fn rules_are_language_scoped() {
        // A Go test convention must not fire on a Rust symbol.
        let (_tmp, index) = workspace(&[("a.rs", "fn TestThing() {}\n")]);
        let kinds = kinds_for(&index, "TestThing");
        assert!(
            !kinds.contains(&EntryKind::Test),
            "Go's TestXxx convention must not apply to Rust: {kinds:?}"
        );
    }

    #[test]
    fn detection_records_which_rule_matched() {
        let (_tmp, index) = workspace(&[("a.rs", "fn main() {}\n")]);
        let entries = Catalog::builtin().unwrap().detect(&index);
        let main_entry = entries
            .iter()
            .find(|e| e.kind == EntryKind::CliMain)
            .unwrap();
        assert!(
            !main_entry.rule.is_empty(),
            "every detection names its rule, so a reader can trace it back"
        );
    }

    #[test]
    fn external_catalogs_can_be_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        crate::vfs::write(
            tmp.path().join("custom.yaml"),
            "- id: custom-handler\n  kind: http-route\n  languages: [rust]\n  matches:\n    name_suffix: _handler\n",
        )
        .unwrap();

        let mut catalog = Catalog::builtin().unwrap();
        let added = catalog.load_dir(tmp.path()).unwrap();
        assert_eq!(added, 1);

        let (_tmp2, index) = workspace(&[("a.rs", "fn login_handler() {}\n")]);
        let entries = catalog.detect(&index);
        assert!(entries.iter().any(|e| e.rule == "custom-handler"));
    }

    #[test]
    fn a_typo_in_a_catalog_is_rejected_not_ignored() {
        // Without deny_unknown_fields a misspelled key would parse fine and produce
        // a rule that silently never matches.
        let yaml =
            "- id: typo\n  kind: cli-main\n  languages: [rust]\n  matches:\n    nmae: main\n";
        let err = serde_yaml::from_str::<Vec<Rule>>(yaml).unwrap_err();
        assert!(err.to_string().contains("nmae"), "got: {err}");
    }

    #[test]
    fn file_prefix_matcher_works() {
        let (_tmp, index) = workspace(&[("test_app.py", "def check_it():\n    pass\n")]);
        let kinds = kinds_for(&index, "check_it");
        assert!(
            kinds.contains(&EntryKind::Test),
            "a function in test_*.py is a test: {kinds:?}"
        );
    }

    #[test]
    fn html_and_xml_entry_points_are_detected() {
        let (_tmp, index) = workspace(&[
            ("index.html", "<div id=\"root\"></div>\n"),
            ("pom.xml", "<project><artifactId id=\"a\"/></project>\n"),
        ]);
        let entries = Catalog::builtin().unwrap().detect(&index);
        assert!(
            entries.iter().any(|e| e.rule == "html-root-mount"),
            "an app mount point is where a page hands over to code: {entries:?}"
        );
        assert!(
            entries.iter().any(|e| e.rule == "xml-maven-project"),
            "a build descriptor is external input: {entries:?}"
        );
    }

    const CLI_PY: &str = "def main():\n    print(\"main\", shared())\n\n\ndef other_main():\n    print(\"other\", shared())\n\n\ndef shared():\n    return 7\n";

    #[test]
    fn a_packages_dunder_main_makes_what_it_calls_an_entry_point() {
        let (_tmp, index) = workspace(&[
            ("mypkg/__init__.py", "\n"),
            (
                "mypkg/__main__.py",
                "from mypkg.cli import main\n\nif __name__ == \"__main__\":\n    main()\n",
            ),
            ("mypkg/cli.py", CLI_PY),
        ]);
        let entries = Catalog::builtin().unwrap().detect(&index);
        let main = index
            .find_symbols("main", None)
            .into_iter()
            .find(|s| s.file.ends_with("cli.py"))
            .unwrap();
        let entry = entries
            .iter()
            .find(|e| e.symbol == main.id && e.kind == EntryKind::CliMain)
            .unwrap_or_else(|| panic!("main is an entry point: {entries:?}"));
        assert!(!entry.rule.is_empty(), "the detection names its basis");
    }

    #[test]
    fn a_call_inside_a_function_of_dunder_main_is_not_itself_an_entry() {
        // Only module-level statements run under `python -m pkg`.
        let (_tmp, index) = workspace(&[
            ("mypkg/__init__.py", "\n"),
            (
                "mypkg/__main__.py",
                "def wrapper():\n    inner()\n\n\ndef inner():\n    pass\n",
            ),
        ]);
        let entries = Catalog::builtin().unwrap().detect(&index);
        let inner = index.find_symbols("inner", None)[0];
        assert!(
            !entries
                .iter()
                .any(|e| e.symbol == inner.id && e.rule.starts_with("python-package-main")),
            "got {entries:?}"
        );
    }

    #[test]
    fn a_setup_py_console_script_is_an_entry_point() {
        let (_tmp, index) = workspace(&[
            (
                "setup.py",
                "from setuptools import setup\n\nsetup(\n    name=\"mypkg\",\n    entry_points={\n        \"console_scripts\": [\n            \"mytool=mypkg.cli:run_tool\",\n        ]\n    },\n)\n",
            ),
            ("mypkg/__init__.py", "\n"),
            ("mypkg/cli.py", "def run_tool():\n    return 7\n"),
        ]);
        let entries = Catalog::builtin().unwrap().detect(&index);
        let run_tool = index
            .find_symbols("run_tool", None)
            .into_iter()
            .find(|s| s.file.ends_with("cli.py"))
            .unwrap();
        let entry = entries
            .iter()
            .find(|e| e.symbol == run_tool.id && e.kind == EntryKind::CliMain)
            .unwrap_or_else(|| panic!("the console script is an entry: {entries:?}"));
        assert!(
            entry.rule.contains("setup.py"),
            "the provenance names the packaging file: {}",
            entry.rule
        );
    }

    #[test]
    fn a_pyproject_scripts_entry_keeps_its_function_reachable() {
        // The probe saw `other_main` flagged for deletion although
        // `[project.scripts]` installs it as a command.
        let (_tmp, index) = workspace(&[
            (
                "pyproject.toml",
                "[project]\nname = \"mypkg\"\nversion = \"0.1.0\"\n\n[project.scripts]\nothertool = \"mypkg.cli:other_main\"\n",
            ),
            ("mypkg/__init__.py", "\n"),
            ("mypkg/cli.py", CLI_PY),
            ("mypkg/extra.py", "def dead():\n    return \"never called\"\n"),
        ]);
        let entries = Catalog::builtin().unwrap().detect(&index);
        let other_main = index.find_symbols("other_main", None)[0];
        let entry = entries
            .iter()
            .find(|e| e.symbol == other_main.id)
            .unwrap_or_else(|| panic!("the scripts table declares other_main: {entries:?}"));
        assert!(
            entry.rule.contains("pyproject.toml"),
            "the provenance names its basis: {}",
            entry.rule
        );

        let graph = crate::analysis::call_graph::CallGraph::build(&index);
        let seeds: Vec<SymbolId> = entries.iter().map(|e| e.symbol).collect();
        let reachable = graph.reachable_from(&seeds);
        assert!(reachable.contains(&other_main.id));
        let shared = index.find_symbols("shared", None)[0];
        assert!(reachable.contains(&shared.id), "what a script calls lives");
        let dead = index.find_symbols("dead", None)[0];
        assert!(!reachable.contains(&dead.id), "truly dead code stays dead");
    }

    #[test]
    fn coverage_gaps_are_reportable() {
        let catalog = Catalog::builtin().unwrap();
        let gaps = languages_without_rules(&catalog);

        for language in Language::ALL {
            let named = gaps.contains(&language.name());
            let has_rules = has_rules_for(&catalog, *language);
            assert_ne!(
                named,
                has_rules,
                "{language} is {} the gap list and {} rules",
                if named { "in" } else { "not in" },
                if has_rules { "has" } else { "has no" }
            );
        }
        // And it is a real report and not an empty one.
        assert!(!gaps.is_empty(), "no gaps at all, so nothing was compared");
        assert!(
            gaps.len() < Language::ALL.len(),
            "every language is a gap, so nothing reads the catalog"
        );
    }
}
