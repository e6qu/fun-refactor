//! Stitching references across the code/config boundary.
//!
//! A Helm value reaches application code by a route no single tool follows: a key in
//! `values.yaml` is named by a `{{ .Values.x }}` action in a template, that action
//! supplies the `value` of a container environment variable, and the program reads
//! that variable by name at runtime. Three files, three languages, one value.
//!
//! Every link here is name-keyed rather than resolved — an environment variable is
//! matched by the string a program passes to `getenv`, which no static analysis can
//! prove refers to the same variable a manifest declares. So every chain carries
//! [`Confidence::NameOnly`] on that hop and says so, rather than presenting a guess
//! as a fact.

use crate::index::Index;
use crate::lang::Language;
use crate::model::{Confidence, SymbolKind};
use crate::parse::Parsers;
use crate::span::LineIndex;
use anyhow::Result;
use std::path::PathBuf;

/// One end-to-end route from configuration into code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    /// The environment variable's name, as declared in the manifest.
    pub env_var: String,
    /// Where the manifest declares it.
    pub declared_in: PathBuf,
    pub declared_line: usize,
    /// The `.Values` path supplying its value, when a template action supplies one.
    pub values_path: Option<Vec<String>>,
    /// Where that path is defined in a values file, when it is.
    pub values_file: Option<PathBuf>,
    /// Every place the program reads the variable.
    pub reads: Vec<EnvRead>,
}

impl Chain {
    /// A chain nothing reads is configuration with no consumer.
    pub fn is_orphaned(&self) -> bool {
        self.reads.is_empty()
    }

    /// Languages the chain spans, manifest included.
    pub fn languages(&self) -> Vec<Language> {
        let mut langs: Vec<Language> = self.reads.iter().map(|r| r.language).collect();
        langs.push(Language::Helm);
        langs.sort();
        langs.dedup();
        langs
    }
}

/// A place in the program that reads an environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvRead {
    pub file: PathBuf,
    pub line: usize,
    pub language: Language,
    /// The call or access as written.
    pub text: String,
    /// Always name-only: the link is a string, not a resolved reference.
    pub confidence: Confidence,
}

/// Find every configuration-to-code chain in the workspace.
pub fn chains(index: &Index) -> Result<Vec<Chain>> {
    let declarations = env_declarations(index)?;
    let reads = env_reads(index)?;

    let mut chains: Vec<Chain> = Vec::new();
    for declaration in declarations {
        let matching: Vec<EnvRead> = reads
            .iter()
            .filter(|r| r.name == declaration.name)
            .map(|r| r.read.clone())
            .collect();

        let values_file = declaration.values_path.as_ref().and_then(|path| {
            values_file_defining(index, &declaration.file, path)
        });

        chains.push(Chain {
            env_var: declaration.name,
            declared_in: declaration.file,
            declared_line: declaration.line,
            values_path: declaration.values_path,
            values_file,
            reads: matching,
        });
    }

    chains.sort_by(|a, b| a.env_var.cmp(&b.env_var));
    chains.dedup();
    Ok(chains)
}

/// The chains touching one environment variable.
pub fn for_variable(index: &Index, name: &str) -> Result<Vec<Chain>> {
    Ok(chains(index)?
        .into_iter()
        .filter(|c| c.env_var == name)
        .collect())
}

/// An environment variable declared by a manifest.
struct Declaration {
    name: String,
    file: PathBuf,
    line: usize,
    values_path: Option<Vec<String>>,
}

/// Environment variables declared in Helm templates and plain manifests.
///
/// The shape looked for is the Kubernetes one: a `name`/`value` pair inside an `env`
/// sequence. `name` gives the variable, and a template action overlapping `value`
/// gives the `.Values` path behind it.
fn env_declarations(index: &Index) -> Result<Vec<Declaration>> {
    let parsers = Parsers::new();
    let mut found = Vec::new();

    for (path, info) in index.files() {
        if !matches!(info.language, Language::Helm | Language::Yaml) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let parsed = parsers.parse(info.language, &source)?;
        let line_index = LineIndex::new(&source);

        let keys: Vec<&crate::model::Symbol> = info
            .symbols
            .iter()
            .filter_map(|id| index.symbol(*id))
            .filter(|s| s.kind == SymbolKind::Key)
            .collect();

        for name_key in keys.iter().filter(|s| s.name == "name") {
            // Only `name` keys that sit under an `env:` list describe a variable.
            if !under_env_list(&keys, name_key) {
                continue;
            }
            let Some(variable) = value_of(name_key, &source) else {
                continue;
            };
            if variable.is_empty() {
                continue;
            }

            // The sibling `value` key, if this entry has a literal or templated one.
            let sibling = keys
                .iter()
                .filter(|s| s.name == "value" && s.qualifier == name_key.qualifier)
                .min_by_key(|s| s.full_span.start.abs_diff(name_key.full_span.end));

            // A `value:` whose entire content is a template action parses as a key
            // with a null value, so its span stops before the action. Match on the
            // line instead, which is where the action actually sits.
            let values_path = sibling.and_then(|value_key| {
                let key_line = line_index.line_col(value_key.name_span.start, &source).line;
                parsed
                    .template_actions
                    .iter()
                    .find(|action| {
                        line_index.line_col(action.start, &source).line == key_line
                    })
                    .and_then(|action| {
                        crate::analysis::provenance::values_paths_in(action.text(&source))
                            .into_iter()
                            .next()
                    })
            });

            found.push(Declaration {
                name: variable,
                file: path.clone(),
                line: line_index.line_col(name_key.name_span.start, &source).line,
                values_path,
            });
        }
    }
    Ok(found)
}

/// Is this key inside an `env:` sequence?
fn under_env_list(keys: &[&crate::model::Symbol], key: &crate::model::Symbol) -> bool {
    // The extractor qualifies a nested key by its parent, and an `env:` list's
    // entries qualify as `env`. Fall back to an ancestor search for deeper shapes.
    if key.qualifier.as_deref() == Some("env") {
        return true;
    }
    let mut current = key.container;
    for _ in 0..8 {
        let Some(id) = current else { return false };
        let Some(parent) = keys.iter().find(|s| s.id == id) else {
            return false;
        };
        if parent.name == "env" {
            return true;
        }
        current = parent.container;
    }
    false
}

/// The scalar written after a key's colon.
fn value_of(key: &crate::model::Symbol, source: &str) -> Option<String> {
    let pair = key.full_span.text(source);
    let after = pair.find(':').map(|i| &pair[i + 1..])?;
    Some(
        after
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches(['"', '\''])
            .to_string(),
    )
}

/// The values file defining a `.Values` path, searched from the template outwards.
fn values_file_defining(index: &Index, template: &PathBuf, path: &[String]) -> Option<PathBuf> {
    let leaf = path.last()?;
    let mut dir = template.parent();
    while let Some(current) = dir {
        for (file, info) in index.files() {
            if file.parent() != Some(current) {
                continue;
            }
            let is_values = file
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("values"));
            if !is_values {
                continue;
            }
            let defines = info
                .symbols
                .iter()
                .filter_map(|id| index.symbol(*id))
                .any(|s| s.kind == SymbolKind::Key && &s.name == leaf);
            if defines {
                return Some(file.clone());
            }
        }
        dir = current.parent();
    }
    None
}

/// A read of an environment variable found in program source.
struct NamedRead {
    name: String,
    read: EnvRead,
}

/// Every environment-variable read in the workspace's code.
///
/// These are matched textually against the well-known accessors of each language,
/// because the variable's name is a string argument rather than a resolvable symbol.
/// A read built from a computed name is invisible here and cannot be otherwise.
fn env_reads(index: &Index) -> Result<Vec<NamedRead>> {
    let mut reads = Vec::new();

    for (path, info) in index.files() {
        let accessors = accessors_for(info.language);
        if accessors.is_empty() {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let line_index = LineIndex::new(&source);

        for accessor in accessors {
            for (offset, _) in source.match_indices(accessor) {
                let rest = &source[offset + accessor.len()..];
                let Some(name) = variable_name_after(rest) else {
                    continue;
                };
                let line = line_index.line_col(offset, &source).line;
                let text = line_index
                    .line_span(line)
                    .map(|s| s.text(&source).trim().to_string())
                    .unwrap_or_default();
                reads.push(NamedRead {
                    name,
                    read: EnvRead {
                        file: path.clone(),
                        line,
                        language: info.language,
                        text,
                        // The link is a string match, never a resolved reference.
                        confidence: Confidence::NameOnly,
                    },
                });
            }
        }
    }
    Ok(reads)
}

/// The accessor prefixes that introduce an environment variable name.
fn accessors_for(language: Language) -> &'static [&'static str] {
    match language {
        Language::Python => &["os.environ.get(", "os.environ[", "os.getenv("],
        Language::Go => &["os.Getenv(", "os.LookupEnv("],
        Language::Rust => &["env::var(", "std::env::var(", "env::var_os("],
        Language::TypeScript | Language::Tsx => &["process.env.", "process.env["],
        Language::Bash => &["${", "$"],
        _ => &[],
    }
}

/// Read the variable name immediately following an accessor.
fn variable_name_after(rest: &str) -> Option<String> {
    let trimmed = rest.trim_start();
    let quoted = trimmed.starts_with('"') || trimmed.starts_with('\'');
    let body = if quoted { &trimmed[1..] } else { trimmed };

    let end = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    let name = &body[..end];

    // Environment variables are conventionally upper case; requiring that keeps a
    // bare `$path` in a shell script from being mistaken for one.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        || !name.chars().any(|c| c.is_ascii_uppercase())
    {
        return None;
    }
    Some(name.to_string())
}

/// Render chains for display.
pub fn format_chains(chains: &[Chain]) -> String {
    let mut out = String::new();
    for chain in chains {
        out.push_str(&format!("{}\n", chain.env_var));
        if let (Some(path), Some(file)) = (&chain.values_path, &chain.values_file) {
            out.push_str(&format!(
                "  from  .Values.{}  ({})\n",
                path.join("."),
                file.display()
            ));
        } else if let Some(path) = &chain.values_path {
            out.push_str(&format!(
                "  from  .Values.{}  (no values file defines it)\n",
                path.join(".")
            ));
        }
        out.push_str(&format!(
            "  set   {}:{}\n",
            chain.declared_in.display(),
            chain.declared_line
        ));
        if chain.reads.is_empty() {
            out.push_str("  read  nothing in this workspace reads it\n");
        }
        for read in &chain.reads {
            out.push_str(&format!(
                "  read  {}:{}  [{}]  {}\n",
                read.file.display(),
                read.line,
                read.confidence.as_str(),
                read.text
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{scan, ScanOptions};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    const CHART: &str = "name: demo\nversion: 0.1.0\n";
    const VALUES: &str = "db:\n  url: postgres://localhost\nreplicas: 2\n";
    const DEPLOYMENT: &str = "spec:\n  containers:\n    - name: app\n      env:\n        - name: DATABASE_URL\n          value: {{ .Values.db.url }}\n";

    #[test]
    fn links_a_values_key_through_an_env_var_into_python() {
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            ("chart/templates/deployment.yaml", DEPLOYMENT),
            ("app/main.py", "import os\n\ndef connect():\n    return os.environ[\"DATABASE_URL\"]\n"),
        ]);

        let chains = chains(&index).unwrap();
        let chain = chains
            .iter()
            .find(|c| c.env_var == "DATABASE_URL")
            .unwrap_or_else(|| panic!("no chain: {:?}", chains.iter().map(|c| &c.env_var).collect::<Vec<_>>()));

        assert_eq!(
            chain.values_path.as_deref(),
            Some(&["db".to_string(), "url".to_string()][..])
        );
        assert!(chain.values_file.is_some(), "values.yaml should be located");
        assert_eq!(chain.reads.len(), 1, "got {:?}", chain.reads);
        assert_eq!(chain.reads[0].language, Language::Python);
    }

    #[test]
    fn the_code_link_is_never_claimed_as_certain() {
        // A program names its variable with a string; nothing can prove it is the
        // one a manifest declares.
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            ("chart/templates/deployment.yaml", DEPLOYMENT),
            ("app/main.py", "import os\nx = os.getenv(\"DATABASE_URL\")\n"),
        ]);
        let chains = chains(&index).unwrap();
        let chain = chains.iter().find(|c| c.env_var == "DATABASE_URL").unwrap();
        assert!(chain
            .reads
            .iter()
            .all(|r| r.confidence == Confidence::NameOnly));
    }

    #[test]
    fn finds_reads_in_every_supported_language() {
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            ("chart/templates/deployment.yaml", DEPLOYMENT),
            ("a/main.py", "import os\nos.environ[\"DATABASE_URL\"]\n"),
            ("a/main.go", "package main\n\nimport \"os\"\n\nfunc main() { _ = os.Getenv(\"DATABASE_URL\") }\n"),
            ("a/main.rs", "fn main() { let _ = std::env::var(\"DATABASE_URL\"); }\n"),
            ("a/app.ts", "export const u = process.env.DATABASE_URL;\n"),
        ]);
        let chains = chains(&index).unwrap();
        let chain = chains.iter().find(|c| c.env_var == "DATABASE_URL").unwrap();

        let langs = chain.languages();
        for expected in [
            Language::Python,
            Language::Go,
            Language::Rust,
            Language::TypeScript,
        ] {
            assert!(langs.contains(&expected), "missing {expected}: {langs:?}");
        }
    }

    #[test]
    fn a_variable_nothing_reads_is_reported_as_orphaned() {
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            ("chart/templates/deployment.yaml", DEPLOYMENT),
            ("app/main.py", "print('unrelated')\n"),
        ]);
        let chains = chains(&index).unwrap();
        let chain = chains.iter().find(|c| c.env_var == "DATABASE_URL").unwrap();
        assert!(chain.is_orphaned());
    }

    #[test]
    fn a_container_name_is_not_mistaken_for_an_env_var() {
        // `- name: app` names a container, not a variable; only `name` keys under an
        // `env:` list count.
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            ("chart/templates/deployment.yaml", DEPLOYMENT),
        ]);
        let chains = chains(&index).unwrap();
        assert!(
            !chains.iter().any(|c| c.env_var == "app"),
            "got {:?}",
            chains.iter().map(|c| &c.env_var).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lower_case_names_are_not_treated_as_environment_variables() {
        assert_eq!(variable_name_after("\"DATABASE_URL\")"), Some("DATABASE_URL".into()));
        assert_eq!(variable_name_after("\"lower_case\")"), None);
        assert_eq!(variable_name_after("path/to/thing"), None);
        assert_eq!(variable_name_after("\"MIXED_case\")"), None);
    }

    #[test]
    fn a_literal_env_value_still_produces_a_chain() {
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            (
                "chart/templates/d.yaml",
                "spec:\n  env:\n    - name: LOG_LEVEL\n      value: debug\n",
            ),
            ("app/main.py", "import os\nos.getenv(\"LOG_LEVEL\")\n"),
        ]);
        let chains = chains(&index).unwrap();
        let chain = chains.iter().find(|c| c.env_var == "LOG_LEVEL").unwrap();
        // No template action, so no .Values path — but the chain into code holds.
        assert!(chain.values_path.is_none());
        assert_eq!(chain.reads.len(), 1);
    }

    #[test]
    fn format_is_readable_and_names_the_uncertainty() {
        let (_tmp, index) = workspace(&[
            ("chart/Chart.yaml", CHART),
            ("chart/values.yaml", VALUES),
            ("chart/templates/deployment.yaml", DEPLOYMENT),
            ("app/main.py", "import os\nos.environ[\"DATABASE_URL\"]\n"),
        ]);
        let text = format_chains(&chains(&index).unwrap());
        assert!(text.contains("DATABASE_URL"), "got:\n{text}");
        assert!(text.contains(".Values.db.url"), "got:\n{text}");
        assert!(text.contains("name-only"), "got:\n{text}");
    }
}
