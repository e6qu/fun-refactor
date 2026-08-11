//! Stitching references across the code/config boundary.
//!
//! A Helm value reaches application code by a route no single tool follows: a key in
//! `values.yaml` is named by a `{{ .Values.x }}` action in a template, that action
//! supplies the `value` of a container environment variable, and the program reads
//! that variable by name at runtime. Three files, three languages, one value.
//!
//! Every link here is name-keyed and not resolved, an environment variable is
//! matched by the string a program passes to `getenv`, which no static analysis can
//! prove refers to the same variable a manifest declares. So every chain carries
//! [`Confidence::NameOnly`] on that hop and says so, instead of presenting a guess
//! as a fact.
//!
//! The manifest end of the chain is read with [`crate::helm`]: the `.Values` path
//! behind a variable lives inside a `{{ ... }}` action, which is masked out before
//! the YAML parse, and a `{{ if }}` around the `value:` decides whether that branch
//! is the one that renders. Both are parsed and not matched by line number.

use crate::helm;
use crate::index::Index;
use crate::lang::Language;
use crate::model::{Confidence, Symbol, SymbolKind};
use crate::parse::Parsers;
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::path::{Path, PathBuf};

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
    /// The template conditional the declaration sits under, when it sits under one.
    ///
    /// A `{{ if }}`-guarded `value:` is one branch of several, so the chain holds
    /// only when its condition does. Two branches produce two chains for the same
    /// variable, each naming its own condition, instead of one that silently picks.
    pub conditional_on: Option<String>,
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
    crate::capabilities::record_workspace(crate::capabilities::Capability::Stitch, index);
    let declarations = env_declarations(index)?;
    let reads = env_reads(index)?;

    let mut chains: Vec<Chain> = Vec::new();
    for declaration in declarations {
        let matching: Vec<EnvRead> = reads
            .iter()
            .filter(|r| r.name == declaration.name)
            .map(|r| r.read.clone())
            .collect();

        let values_file = declaration
            .values_path
            .as_ref()
            .and_then(|path| values_file_defining(index, &declaration.file, path));

        chains.push(Chain {
            env_var: declaration.name,
            declared_in: declaration.file,
            declared_line: declaration.line,
            values_path: declaration.values_path,
            values_file,
            conditional_on: declaration.conditional_on,
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
    conditional_on: Option<String>,
}

/// Environment variables declared in Helm templates and plain manifests.
///
/// The shape looked for is the Kubernetes one: a `name`/`value` pair inside an `env`
/// sequence. `name` gives the variable, and the template actions written after the
/// sibling `value:` give the `.Values` path behind it.
fn env_declarations(index: &Index) -> Result<Vec<Declaration>> {
    let parsers = Parsers::new();
    let mut found = Vec::new();

    for (path, info) in index.files() {
        if !matches!(info.language, Language::Helm | Language::Yaml) {
            continue;
        }
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        let parsed = parsers.parse(info.language, &source)?;
        let template = helm::Template::of(&source, &parsed);
        let line_index = LineIndex::new(&source);

        let keys: Vec<&Symbol> = info
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
            let line = line_index.line_col(name_key.name_span.start, &source).line;

            let values = value_keys(&keys, name_key, source.len());
            if values.is_empty() {
                // `valueFrom:`, or a variable whose value the manifest does not set.
                found.push(Declaration {
                    name: variable,
                    file: path.clone(),
                    line,
                    values_path: None,
                    conditional_on: describe(&template.conditions_at(name_key.name_span.start)),
                });
                continue;
            }

            // Each `value:` is a branch: `{{ if }}` around one of them means this
            // entry has several possible values, and which renders is the template's
            // decision. Every branch is reported, with the condition that selects it.
            for value_key in values {
                found.push(Declaration {
                    name: variable.clone(),
                    file: path.clone(),
                    line,
                    values_path: templated_values_path(&template, &line_index, &source, value_key),
                    conditional_on: describe(&template.conditions_at(value_key.name_span.start)),
                });
            }
        }
    }
    Ok(found)
}

/// The `value:` keys belonging to one `name:` entry of an `env:` list.
///
/// A list item is not its own container in the index, so the entry is bounded by
/// the next `name:` key at the same level instead.
fn value_keys<'a>(keys: &[&'a Symbol], name_key: &Symbol, end_of_file: usize) -> Vec<&'a Symbol> {
    let next_entry = keys
        .iter()
        .filter(|s| s.name == "name" && s.qualifier == name_key.qualifier)
        .map(|s| s.full_span.start)
        .filter(|start| *start > name_key.full_span.start)
        .min()
        .unwrap_or(end_of_file);
    keys.iter()
        .copied()
        .filter(|s| s.name == "value" && s.qualifier == name_key.qualifier)
        .filter(|s| s.full_span.start >= name_key.full_span.end && s.full_span.start < next_entry)
        .collect()
}

/// The `.Values` path a `value:` takes its content from.
///
/// A `value:` whose whole content is a template action parses as a key with a null
/// value, its span stops at the colon, because the action's bytes are masked. The
/// path therefore comes from the actions written after the key, on its own line.
fn templated_values_path(
    template: &helm::Template,
    lines: &LineIndex,
    source: &str,
    value_key: &Symbol,
) -> Option<Vec<String>> {
    let line = lines.line_col(value_key.name_span.start, source).line;
    let line_span = lines.line_span(line)?;
    let after_key = Span::new(
        value_key.name_span.end,
        line_span.end.max(value_key.name_span.end),
    );
    template
        .actions_in(after_key)
        .into_iter()
        // A control action on the line decides which text renders; it does not
        // supply the value itself.
        .filter(|(_, action)| action.kind.opens_region().is_none())
        .flat_map(|(index, _)| template.values_paths_of(index))
        .next()
}

/// The conditions guarding a point, as one phrase.
fn describe(guards: &[helm::Guard]) -> Option<String> {
    (!guards.is_empty()).then(|| {
        guards
            .iter()
            .map(helm::Guard::describe)
            .collect::<Vec<_>>()
            .join(" and ")
    })
}

/// Is this key inside an `env:` sequence?
fn under_env_list(keys: &[&Symbol], key: &Symbol) -> bool {
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
fn value_of(key: &Symbol, source: &str) -> Option<String> {
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
fn values_file_defining(index: &Index, template: &Path, path: &[String]) -> Option<PathBuf> {
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
/// because the variable's name is a string argument and not a resolvable symbol.
/// A read built from a computed name is invisible here and cannot be otherwise.
fn env_reads(index: &Index) -> Result<Vec<NamedRead>> {
    let mut reads = Vec::new();

    for (path, info) in index.files() {
        let accessors = accessors_for(info.language);
        if accessors.is_empty() {
            continue;
        }
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        let line_index = LineIndex::new(&source);

        for accessor in accessors {
            for (offset, _) in source.match_indices(accessor.prefix) {
                let rest = &source[offset + accessor.prefix.len()..];
                let Some(name) = variable_name_after(rest, accessor.skip_arguments) else {
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

/// One way a language names an environment variable.
struct Accessor {
    /// The text that introduces the read.
    prefix: &'static str,
    /// Arguments standing between the prefix and the name.
    ///
    /// Every accessor the first five languages use puts the name first, and the reader
    /// was written to take it from there. Zig's does not: `getEnvVarOwned` is passed an
    /// allocator before the name, so reading the first argument found `allocator`, which
    /// is lower case, which the name filter dropped, a read that went missing without
    /// saying so.
    skip_arguments: usize,
}

const fn first(prefix: &'static str) -> Accessor {
    Accessor {
        prefix,
        skip_arguments: 0,
    }
}

const fn past(prefix: &'static str, skip_arguments: usize) -> Accessor {
    Accessor {
        prefix,
        skip_arguments,
    }
}

const PYTHON: &[Accessor] = &[
    first("os.environ.get("),
    first("os.environ["),
    first("os.getenv("),
];
const GO: &[Accessor] = &[first("os.Getenv("), first("os.LookupEnv(")];
const RUST: &[Accessor] = &[
    first("env::var("),
    first("std::env::var("),
    first("env::var_os("),
];
const JAVA: &[Accessor] = &[first("System.getenv("), first("System.getenv().get(")];
const ZIG: &[Accessor] = &[
    past("std.process.getEnvVarOwned(", 1),
    past("std.process.hasEnvVar(", 1),
    first("std.process.hasEnvVarConstant("),
    first("std.posix.getenv("),
    first("std.c.getenv("),
];
const TYPESCRIPT: &[Accessor] = &[first("process.env."), first("process.env[")];
const BASH: &[Accessor] = &[first("${"), first("$")];

/// The accessors that introduce an environment variable name.
fn accessors_for(language: Language) -> &'static [Accessor] {
    match language {
        Language::Python => PYTHON,
        Language::Go => GO,
        Language::Rust => RUST,
        Language::Java => JAVA,
        Language::Zig => ZIG,
        Language::TypeScript | Language::Tsx => TYPESCRIPT,
        Language::Bash => BASH,
        Language::Html
        | Language::Css
        | Language::Scss
        | Language::Hcl
        | Language::Yaml
        | Language::Helm
        | Language::Xml
        | Language::Markdown => &[],
    }
}

/// Does any program written in this language read the environment in a way stitch sees?
pub fn reads_environment(language: Language) -> bool {
    !accessors_for(language).is_empty()
}

/// Read the variable name following an accessor, past any arguments before it.
fn variable_name_after(rest: &str, skip_arguments: usize) -> Option<String> {
    let mut rest = rest;
    for _ in 0..skip_arguments {
        // Only a flat argument list is followed. A name reached past a nested call or a
        // struct literal is not read and not guessed at, on the same footing as a
        // name computed at run time.
        let comma = rest.find(',')?;
        if rest[..comma].contains(['(', ')', '{', '}']) {
            return None;
        }
        rest = &rest[comma + 1..];
    }
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
        if let Some(condition) = &chain.conditional_on {
            out.push_str(&format!("  only  when `{condition}` renders\n"));
        }
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
            crate::vfs::write(&path, content).unwrap();
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
            (
                "app/main.py",
                "import os\n\ndef connect():\n    return os.environ[\"DATABASE_URL\"]\n",
            ),
        ]);

        let chains = chains(&index).unwrap();
        let chain = chains
            .iter()
            .find(|c| c.env_var == "DATABASE_URL")
            .unwrap_or_else(|| {
                panic!(
                    "no chain: {:?}",
                    chains.iter().map(|c| &c.env_var).collect::<Vec<_>>()
                )
            });

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
            (
                "app/main.py",
                "import os\nx = os.getenv(\"DATABASE_URL\")\n",
            ),
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
        assert_eq!(
            variable_name_after("\"DATABASE_URL\")", 0),
            Some("DATABASE_URL".into())
        );
        assert_eq!(variable_name_after("\"lower_case\")", 0), None);
        assert_eq!(variable_name_after("path/to/thing", 0), None);
        assert_eq!(variable_name_after("\"MIXED_case\")", 0), None);
    }

    #[test]
    fn a_name_behind_an_allocator_is_still_read() {
        // Zig passes the allocator first, so the name is the second argument. Taking
        // the first found `allocator`, which the upper-case filter then dropped.
        assert_eq!(
            variable_name_after("allocator, \"DATABASE_URL\")", 1),
            Some("DATABASE_URL".into())
        );
        assert_eq!(
            variable_name_after(" gpa.allocator(), \"PORT\")", 1),
            None,
            "an argument that is itself a call is not followed"
        );
        assert_eq!(variable_name_after("allocator)", 1), None);
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
        // No template action, so no .Values path, but the chain into code holds.
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
