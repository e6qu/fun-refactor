//! Entry-point detection driven by declarative catalogs.
//!
//! funveil detected entry points with heuristics hardcoded in Rust. Here the rules
//! are data: one YAML catalog per framework, following the shape CodeQL uses for
//! models-as-data and OWASP noir uses for endpoint extraction. Adding Flask or Axum
//! support means adding rows, not code.
//!
//! An entry point is a tagged symbol, so it feeds directly into call-graph
//! reachability: "what is reachable from an HTTP handler" is a graph walk from
//! everything tagged `http-route`.

use crate::index::Index;
use crate::lang::Language;
use crate::model::{Symbol, SymbolId};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// What kind of entry point this is. A closed vocabulary so queries can rely on it.
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
///
/// Orthogonal to [`EntryKind`], mirroring CodeQL's threat-model split: the same
/// handler kind means something different depending on who can reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreatModel {
    Remote,
    Local,
    None,
}

/// One rule from a catalog.
///
/// Unknown fields are rejected: a typo in a catalog would otherwise be ignored,
/// leaving a rule that silently never matches.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Human-readable rule name.
    pub id: String,
    pub kind: EntryKind,
    #[serde(default = "default_threat")]
    pub threat_model: ThreatModel,
    /// Languages this rule applies to.
    pub languages: Vec<String>,
    #[serde(default)]
    pub matches: Matcher,
    /// `manual` for hand-written rules, `generated` for derived ones.
    #[serde(default = "default_provenance")]
    pub provenance: String,
}

fn default_threat() -> ThreatModel {
    ThreatModel::None
}

fn default_provenance() -> String {
    "manual".to_string()
}

/// Conditions a symbol must meet. All present conditions must hold.
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
    /// Symbol kind, e.g. function, block, key.
    #[serde(default)]
    pub symbol_kind: Option<String>,
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
    /// An annotation written immediately above the symbol, without its punctuation:
    /// `test` matches Rust's `#[test]` and `#[tokio::test]`, Python's `@pytest.mark`
    /// and Java's `@Test`.
    ///
    /// This is how a test declares itself in most languages, and a name convention is
    /// not: ripgrep's tests are called `backslash`, `tab` and `carriage`. Matching the
    /// name alone left 141 of its 296 test functions looking like dead code.
    #[serde(default)]
    pub annotated_with: Option<String>,
    /// The symbol is called from the module's `if __name__ == "__main__":` block.
    ///
    /// Every other language here says its entry point is a function called `main`, and
    /// the rules were written that way. Python's is not a function at all — it is a
    /// statement, and what it calls can be named anything. A script whose guard calls
    /// `cli()` reported no entry point whatsoever.
    #[serde(default)]
    pub called_from_main_guard: Option<bool>,
    /// A directive at the top of the file, or at the top of the symbol's own body.
    ///
    /// `"use server"` marks a Next.js server action: an exported function the framework
    /// makes reachable over the network, called by nothing in the source. It is the same
    /// case as Java's `@RestController` and pytest's fixtures, and unlike Next.js's other
    /// conventions it is not a filename — `components/cart/actions.ts` is an ordinary
    /// name, so no `file_name` rule can find it.
    #[serde(default)]
    pub file_directive: Option<String>,
    /// The annotation's first string argument must start with this.
    ///
    /// A decorator's name is not unique across libraries: `@patch` is `unittest.mock`'s
    /// far more often than it is FastAPI's. What separates them is what the decorator
    /// names — a URL path or a module — so a route rule asks for the path.
    ///
    /// Only meaningful alongside `annotated_with`, and rejected without it.
    #[serde(default)]
    pub annotation_argument_prefix: Option<String>,
}

/// A detected entry point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrypoint {
    pub symbol: SymbolId,
    pub kind: EntryKind,
    pub threat_model: ThreatModel,
    /// The catalog rule that matched, so a surprising result can be traced back.
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
///
/// `deny_unknown_fields` catches a misspelled key; this catches a well-spelled one in a
/// combination that has no meaning. Such a rule would parse, load, and match nothing —
/// which reads exactly like a framework that is covered and simply absent.
fn check_rules(rules: &[Rule]) -> Result<()> {
    for rule in rules {
        if rule.matches.annotation_argument_prefix.is_some()
            && rule.matches.annotated_with.is_none()
        {
            anyhow::bail!(
                "rule '{}' asks for an annotation argument without naming the annotation",
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
        // One read per symbol rather than one per rule. Three of the predicates below
        // need the file's text, and asking each of them separately read the whole file
        // once for every rule in the catalogue — on `vuejs/core`, 34,611 symbols against
        // the rules that apply to TypeScript, which doubled the time this command takes.
        // The index groups a file's symbols together, so remembering the last one is
        // enough; a file that cannot be read is retried rather than remembered.
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
        found.sort_by_key(|e| (e.kind, e.symbol));
        found.dedup_by_key(|e| (e.symbol, e.kind));
        found
    }
}

/// The roots a reachability question starts from.
///
/// This is a type rather than a `&[SymbolId]` because an empty slice is a legal value
/// with a catastrophic meaning: nothing is reachable, so everything not exported reads
/// as dead. The playground shipped exactly that — twenty symbols reported dead in the
/// browser that the terminal reported live, including every `#[test]` function —
/// because one caller passed `&[]` where the other passed a detected catalog, and both
/// type-checked.
///
/// Now a caller has to say which it means. There is no way to end up with no roots by
/// omission; [`Entrypoints::none`] exists, but you have to ask for it by name.
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
        Entrypoints(catalog.detect(index).iter().map(|e| e.symbol).collect())
    }

    /// Exactly these symbols and no others. For asking what a particular root
    /// reaches, which is a different question from what the workspace runs.
    pub fn exactly(roots: &[SymbolId]) -> Self {
        Entrypoints(roots.to_vec())
    }

    /// No roots at all: only exported symbols anchor reachability. Correct for a
    /// workspace that is a library and nothing else.
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
    if !rule
        .languages
        .iter()
        .any(|l| l == symbol.language.name() || l == "*")
    {
        return false;
    }

    let m = &rule.matches;
    let file_name = symbol
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    // An empty matcher would tag every symbol in the language, which is never what
    // a catalog author means.
    let has_condition = m.name.is_some()
        || m.name_prefix.is_some()
        || m.name_suffix.is_some()
        || m.file_name.is_some()
        || m.path_contains.is_some()
        || m.file_suffix.is_some()
        || m.file_prefix.is_some()
        || m.annotated_with.is_some()
        || m.called_from_main_guard.is_some()
        || m.file_directive.is_some();
    if !has_condition {
        return false;
    }

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
    if let Some(kind) = &m.symbol_kind {
        if symbol.kind.as_str() != kind {
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
/// Is `symbol` annotated with `name` — `#[name]`, `#[path::name]` or `@name`?
///
/// Read from the bytes above the definition rather than from a captured fact,
/// because an attribute is not part of the symbol in any grammar here and adding it
/// to every language's queries would be a larger change than this earns. Only the
/// lines immediately above are considered, so a `#[test]` four declarations up does
/// not leak onto this one.
/// Is an annotation with this name written on the symbol?
///
/// Two shapes, because the languages disagree about where an annotation lives. Rust
/// and Python put it on its own line *above* the definition, so the search walks
/// backwards through the run of `#[…]` and `@…` lines. Java puts it **inside** the
/// declaration, in the `modifiers` node — `@Test public void f()` — so it is within the
/// symbol's own span and a backwards search from the start of that span never sees it.
/// Reading only above it made every `@Test` method in the language look like dead code.
///
/// Public because a recipe's `annotated-with=` predicate is the same question, and the
/// point of reusing the matcher is that the two cannot drift apart.
pub fn annotated_with(symbol: &Symbol, name: &str) -> bool {
    annotation_on(symbol, name).is_some()
}

/// The annotation's own text, so a caller can ask about its arguments as well as its
/// name. [`annotated_with`] is this question with the answer thrown away.
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

    // What sits before the symbol on its own line is part of the declaration, not a line
    // before it: `export class C` and `pub async fn f` put a modifier there, and reading
    // it as a preceding line ended the run before it reached the annotation above. So the
    // search starts at the beginning of the declaration's line.
    //
    // Unless that text opens or closes something, in which case the symbol is nested in
    // whatever the annotation above actually annotates, and does not carry it: the
    // `payload` in `@KafkaListener void consume(String payload)` is not a queue consumer,
    // and the `inner` in `fn outer() { fn inner()` is not `outer`'s `#[test]`.
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
///
/// A decorator's *name* is not unique across libraries. `@patch` is `unittest.mock`'s
/// far more often than it is FastAPI's, and matching the name alone tagged twenty-two
/// of `psf/black`'s test methods as remotely reachable HTTP routes. What separates the
/// two is the argument: a route decorator names a URL path, and a mock names a module.
///
/// A path held in a constant — `@app.get(PETS)` — is not matched. That is a real gap
/// rather than a hidden one: the rule asks for something it can read, and says so.
fn annotation_argument_starts_with(annotation: &str, prefix: &str) -> bool {
    let Some((_, args)) = annotation.split_once('(') else {
        return false;
    };
    args.trim_start()
        .strip_prefix(['"', '\''])
        .is_some_and(|literal| literal.starts_with(prefix))
}

/// Is this symbol called from its module's `if __name__ == "__main__":` block?
///
/// The guard is a statement rather than a declaration, so there is no symbol to match a
/// name against — which is why every other rule here could be written as a name and
/// this one could not. What it calls is the program's starting point whatever it is
/// called, and reporting nothing for a script that has one is the wrong answer to the
/// only question `fr entrypoints` asks.
///
/// Direct calls only: `cli()` inside the guard, not a call made by something the guard
/// calls. The second is reachability, which the call graph answers, and folding it in
/// here would tag half a program as an entry point.
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

/// Is this symbol under a directive — at the top of its file, or of its own body?
///
/// Both forms are real. `"use server"` at the top of a file makes every export in it a
/// server action; the same string at the top of one function body marks only that one.
/// Quoted either way, and the first statement either way, which is what keeps a mention
/// of the words in a comment or a string from counting.
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
///
/// `#[tokio::test]`, `@pytest.mark.asyncio` and `@org.junit.jupiter.api.Test` all end
/// in the bare name, so the qualification is dropped and the arguments with it.
///
/// The arguments go first and the qualifier second, because the arguments may contain
/// dots of their own: `@GetMapping(Routes.PETS)` and `@app.route("/v1.0/status")` name
/// `GetMapping` and `route`, not `PETS` and `0/status")`.
fn names_the_annotation(fragment: &str, name: &str) -> bool {
    // Whitespace ends the name as surely as a bracket does: inside a Java declaration
    // the annotation is followed by a newline and the modifiers, not by a delimiter.
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
        .any(|r| r.languages.iter().any(|l| l == language.name() || l == "*"))
}

/// Languages with no entry-point rules at all, so gaps in coverage are visible.
pub fn languages_without_rules(catalog: &Catalog) -> Vec<&'static str> {
    Language::ALL
        .iter()
        .filter(|lang| {
            !catalog
                .rules
                .iter()
                .any(|r| r.languages.iter().any(|l| l == lang.name() || l == "*"))
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
            scanned.files.push(SourceFile {
                language: crate::lang::detect(&path).unwrap(),
                path,
            });
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

    #[test]
    fn terraform_variables_are_infra_inputs() {
        let (_tmp, index) =
            workspace(&[("main.tf", "variable \"region\" {\n  type = string\n}\n")]);
        let kinds = kinds_for(&index, "region");
        assert!(
            kinds.contains(&EntryKind::InfraInput),
            "a root-module variable is externally settable: {kinds:?}"
        );
    }

    #[test]
    fn an_empty_matcher_never_matches_everything() {
        // A rule with no conditions would tag every symbol; that is always a
        // catalog authoring mistake, so it must match nothing.
        let rule = Rule {
            id: "bad".into(),
            kind: EntryKind::CliMain,
            threat_model: ThreatModel::None,
            languages: vec!["rust".into()],
            matches: Matcher::default(),
            provenance: "manual".into(),
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
            "every detection must name its rule so it can be traced"
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

    #[test]
    fn coverage_gaps_are_reportable() {
        // This used to assert that each gap's *name* was a non-empty string, which is
        // true of every `&'static str` in the enum and would have passed had the
        // function returned nothing, everything, or the wrong languages entirely. The
        // report tells a reader which languages have no entry-point rules, and the only
        // way that is worth printing is if it agrees with which languages have none.
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
        // And it is a real report rather than an empty one: some language has no rules
        // and some language does, or the agreement above is vacuous.
        assert!(!gaps.is_empty(), "no gaps at all, so nothing was compared");
        assert!(
            gaps.len() < Language::ALL.len(),
            "every language is a gap, so the catalog is not being read"
        );
    }
}
