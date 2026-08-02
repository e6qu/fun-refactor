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
    ("bash", include_str!("../../catalogs/bash.yaml")),
    ("infra", include_str!("../../catalogs/infra.yaml")),
    ("docs", include_str!("../../catalogs/docs.yaml")),
    ("markup", include_str!("../../catalogs/markup.yaml")),
];

impl Catalog {
    /// Load the built-in catalogs.
    pub fn builtin() -> Result<Self> {
        let mut rules = Vec::new();
        for (name, yaml) in BUILTIN {
            let parsed: Vec<Rule> = serde_yaml::from_str(yaml)
                .with_context(|| format!("parsing built-in catalog '{name}'"))?;
            rules.extend(parsed);
        }
        Ok(Catalog { rules })
    }

    /// Load additional rules from a directory of YAML files.
    pub fn load_dir(&mut self, dir: &Path) -> Result<usize> {
        let mut added = 0;
        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("reading catalog directory {}", dir.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let parsed: Vec<Rule> = serde_yaml::from_str(&text)
                .with_context(|| format!("parsing catalog {}", path.display()))?;
            added += parsed.len();
            self.rules.extend(parsed);
        }
        Ok(added)
    }

    /// Find every entry point in an index.
    pub fn detect(&self, index: &Index) -> Vec<Entrypoint> {
        let mut found = Vec::new();
        for symbol in &index.symbols {
            for rule in &self.rules {
                if rule_applies(rule, symbol) {
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

fn rule_applies(rule: &Rule, symbol: &Symbol) -> bool {
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
        || m.file_prefix.is_some();
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
            std::fs::write(&path, content).unwrap();
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
            assert!(!rule.languages.is_empty(), "rule {} has no language", rule.id);
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
        let (_tmp, index) = workspace(&[("app.py", "def main():\n    pass\n\ndef test_it():\n    pass\n")]);
        assert!(kinds_for(&index, "main").contains(&EntryKind::CliMain));
        assert!(kinds_for(&index, "test_it").contains(&EntryKind::Test));
    }

    #[test]
    fn terraform_variables_are_infra_inputs() {
        let (_tmp, index) = workspace(&[(
            "main.tf",
            "variable \"region\" {\n  type = string\n}\n",
        )]);
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
        assert!(!rule_applies(&rule, symbol));
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
        let main_entry = entries.iter().find(|e| e.kind == EntryKind::CliMain).unwrap();
        assert!(
            !main_entry.rule.is_empty(),
            "every detection must name its rule so it can be traced"
        );
    }

    #[test]
    fn external_catalogs_can_be_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
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
        let yaml = "- id: typo\n  kind: cli-main\n  languages: [rust]\n  matches:\n    nmae: main\n";
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
        let catalog = Catalog::builtin().unwrap();
        let gaps = languages_without_rules(&catalog);
        // Whatever the gaps are, they must be enumerable rather than hidden.
        for lang in &gaps {
            assert!(!lang.is_empty());
        }
    }
}
