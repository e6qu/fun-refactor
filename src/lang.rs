//! Language identity and dialect detection.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// A source language, at the granularity that matters for parsing and refactoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Go,
    Zig,
    Java,
    TypeScript,
    Tsx,
    Python,
    Bash,
    Html,
    Css,
    Scss,
    Sass,
    Hcl,
    Yaml,
    Helm,
    Xml,
    Markdown,
    /// JSON, and the JSON syntax HCL also has.
    Json,
}

/// Broad language class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LanguageClass {
    /// Imperative code with functions, scopes and call sites.
    Imperative,
    /// Declarative configuration and markup with string-keyed references.
    Config,
}

impl Language {
    pub const ALL: &'static [Language] = &[
        Language::Rust,
        Language::Go,
        Language::Zig,
        Language::Java,
        Language::TypeScript,
        Language::Tsx,
        Language::Python,
        Language::Bash,
        Language::Html,
        Language::Css,
        Language::Scss,
        Language::Sass,
        Language::Hcl,
        Language::Json,
        Language::Yaml,
        Language::Helm,
        Language::Xml,
        Language::Markdown,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Zig => "zig",
            Language::Java => "java",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::Python => "python",
            Language::Bash => "bash",
            Language::Html => "html",
            Language::Css => "css",
            Language::Scss => "scss",
            Language::Sass => "sass",
            Language::Hcl => "hcl",
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Helm => "helm",
            Language::Xml => "xml",
            Language::Markdown => "markdown",
        }
    }

    pub fn class(&self) -> LanguageClass {
        match self {
            Language::Rust
            | Language::Go
            | Language::Zig
            | Language::Java
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
            | Language::Bash => LanguageClass::Imperative,
            Language::Html
            | Language::Css
            | Language::Scss
            | Language::Sass
            | Language::Hcl
            | Language::Json
            | Language::Yaml
            | Language::Helm
            | Language::Xml
            | Language::Markdown => LanguageClass::Config,
        }
    }

    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::Go => &["go"],
            Language::Zig => &["zig"],
            Language::Java => &["java"],
            // JavaScript is parsed by the TypeScript grammar, which is a superset of it, and
            // read by the same queries.
            Language::TypeScript => &["ts", "mts", "cts", "js", "mjs", "cjs"],
            Language::Tsx => &["tsx", "jsx"],
            Language::Python => &["py", "pyi"],
            Language::Bash => &["sh", "bash"],
            Language::Html => &["html", "htm"],
            Language::Css => &["css"],
            Language::Scss => &["scss"],
            // The indented syntax, which is not SCSS: every block and every statement
            // ends differently, so it has a grammar of its own.
            Language::Sass => &["sass"],
            Language::Hcl => &["tf", "tfvars", "hcl"],
            Language::Json => &["json"],
            Language::Yaml => &["yaml", "yml"],
            // Helm shares YAML extensions and is otherwise distinguished by chart
            // layout, but `.tpl` is unambiguously a Helm template file.
            Language::Helm => &["tpl"],
            Language::Xml => &["xml"],
            Language::Markdown => &["md", "markdown", "mdown", "mkd"],
        }
    }

    /// Does a name resolve across every file in the same directory?
    pub fn resolves_by_directory(&self) -> bool {
        matches!(self, Language::Hcl)
    }

    /// Is every member of a value reached through a receiver written before it?
    pub fn members_always_have_a_receiver(&self) -> bool {
        matches!(
            self,
            Language::Go
                | Language::TypeScript
                | Language::Tsx
                | Language::Python
                | Language::Zig
                | Language::Rust
        )
    }

    /// Is a package here a directory, so that top-level declarations are visible unqualified
    /// from every file beside them?
    pub fn packages_by_directory(&self) -> bool {
        matches!(self, Language::Go)
    }

    /// Does an import run the other file's definitions here, under their own names?
    pub fn splices_sourced_files(&self) -> bool {
        matches!(self, Language::Bash)
    }

    pub fn from_name(name: &str) -> Option<Language> {
        Language::ALL
            .iter()
            .copied()
            .find(|l| l.name() == name.to_ascii_lowercase())
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `pad` and not `write_str`, so a width in the format string is honoured.
        f.pad(self.name())
    }
}

/// Detect a file's language from its path.
pub fn detect(path: &Path) -> Option<Language> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();

    let by_ext = Language::ALL
        .iter()
        .copied()
        .find(|l| l.extensions().contains(&ext.as_str()))?;

    if by_ext == Language::Yaml && is_helm_path(path) {
        return Some(Language::Helm);
    }
    Some(by_ext)
}

/// Well-known Helm chart files and the `templates/` directory identify a chart.
fn is_helm_path(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if matches!(file_name.as_str(), "chart.yaml" | "values.yaml") {
        return true;
    }

    // Any YAML sitting beside a Chart.yaml belongs to that chart, charts routinely carry
    // values-prod.yaml and friends.
    if let Some(dir) = path.parent() {
        if crate::vfs::exists(dir.join("Chart.yaml")) || crate::vfs::exists(dir.join("chart.yaml"))
        {
            return true;
        }
    }

    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("templates"))
    }) && path
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| s == "charts"))
        || has_sibling_chart_yaml(path)
        || writes_template_actions(path)
}

/// A file under `templates/` written in Go template syntax, with no `Chart.yaml` above it.
fn writes_template_actions(path: &Path) -> bool {
    if !under_templates_directory(path) {
        return false;
    }
    crate::vfs::read_to_string(path).is_ok_and(|text| holds_template_action(&text))
}

/// Is any ancestor directory named `templates`?
fn under_templates_directory(path: &Path) -> bool {
    let mut dir = path.parent();
    while let Some(here) = dir {
        if here
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("templates"))
        {
            return true;
        }
        dir = here.parent();
    }
    false
}

/// Does this text hold a Go template action, the syntax Helm charts are written in?
fn holds_template_action(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "include", "if", "range", "with", "template", "define", "end", "else", "toYaml", "printf",
        "quote", "required", "tpl", "default", "nindent", "indent",
    ];
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return false;
        };
        let action = after[..end].trim().trim_start_matches('-').trim_start();
        if action.starts_with('.')
            || action.starts_with('$')
            || KEYWORDS.iter().any(|word| action.starts_with(word))
        {
            return true;
        }
        rest = &after[end + 2..];
    }
    false
}

/// The chart directory a Helm file belongs to: the nearest ancestor with a Chart.yaml.
pub fn is_css_module(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".module.css", ".module.scss", ".module.sass"]
                .iter()
                .any(|suffix| lower.ends_with(suffix))
        })
}

/// A chart's values are its own.
pub fn chart_root(path: &Path) -> Option<&Path> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if crate::vfs::exists(d.join("Chart.yaml")) || crate::vfs::exists(d.join("chart.yaml")) {
            return Some(d);
        }
        dir = d.parent();
    }
    // No metadata anywhere above.
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("templates"))
        {
            return d.parent();
        }
        dir = d.parent();
    }
    None
}

/// A file under `<chart>/templates/` where `<chart>/Chart.yaml` exists is a Helm template.
fn has_sibling_chart_yaml(path: &Path) -> bool {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("templates"))
        {
            if let Some(chart_root) = d.parent() {
                return crate::vfs::exists(chart_root.join("Chart.yaml"))
                    || crate::vfs::exists(chart_root.join("chart.yaml"));
            }
        }
        dir = d.parent();
    }
    false
}

/// Which language boundaries a reference may resolve across.
pub fn may_resolve_across(from: Language, to: Language, t: crate::model::SymbolKind) -> bool {
    use crate::model::SymbolKind as K;
    use Language::*;

    if from == to {
        return true;
    }

    match (from, to) {
        // TSX *is* TypeScript with JSX; a `.tsx` file imports from `.ts` constantly.
        (TypeScript, Tsx) | (Tsx, TypeScript) => true,

        // SCSS compiles to CSS and the two share a selector namespace: a class
        // declared in a theme is the same class the stylesheet declares.
        (Css, Scss) | (Scss, Css) | (Css, Sass) | (Sass, Css) | (Scss, Sass) | (Sass, Scss) => true,

        // Markup names a style rule by class or id.
        (Html | Xml | Tsx | TypeScript | Markdown, Css | Scss | Sass) => {
            matches!(t, K::Selector | K::Property)
        }

        // A Helm template names a key in its values file; a values file is YAML.
        (Helm, Yaml) | (Yaml, Helm) => matches!(t, K::Key),

        // A template or a manifest names an element by id, and markup declares them.
        (Html | Xml | Tsx | TypeScript, Html | Xml) => matches!(t, K::ElementId),

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_by_extension() {
        let cases = [
            ("a/b/main.rs", Language::Rust),
            ("main.go", Language::Go),
            ("build.zig", Language::Zig),
            ("src/index.ts", Language::TypeScript),
            ("src/App.tsx", Language::Tsx),
            ("app.py", Language::Python),
            ("deploy.sh", Language::Bash),
            ("index.html", Language::Html),
            ("style.css", Language::Css),
            ("style.scss", Language::Scss),
            ("main.tf", Language::Hcl),
            ("terraform.tfvars", Language::Hcl),
            ("config.yaml", Language::Yaml),
            ("pom.xml", Language::Xml),
            ("README.md", Language::Markdown),
        ];
        for (path, expected) in cases {
            assert_eq!(detect(Path::new(path)), Some(expected), "path: {path}");
        }
    }

    #[test]
    fn tsx_and_ts_are_distinct_languages() {
        // They use different tree-sitter grammars, so they must not be conflated.
        assert_ne!(Language::TypeScript, Language::Tsx);
        assert_eq!(detect(Path::new("a.ts")), Some(Language::TypeScript));
        assert_eq!(detect(Path::new("a.tsx")), Some(Language::Tsx));
    }

    #[test]
    fn unknown_extensions_are_none() {
        assert_eq!(detect(Path::new("binary.bin")), None);
        assert_eq!(detect(Path::new("no_extension")), None);
    }

    #[test]
    fn tpl_files_are_helm() {
        // `_helpers.tpl` is where named templates live; without this it belongs to no
        // language and cannot be written to.
        assert_eq!(
            detect(Path::new("chart/templates/_helpers.tpl")),
            Some(Language::Helm)
        );
    }

    #[test]
    fn chart_files_detected_as_helm() {
        assert_eq!(
            detect(Path::new("mychart/Chart.yaml")),
            Some(Language::Helm)
        );
        assert_eq!(
            detect(Path::new("mychart/values.yaml")),
            Some(Language::Helm)
        );
        // A plain YAML file elsewhere stays YAML.
        assert_eq!(detect(Path::new("ci/config.yaml")), Some(Language::Yaml));
    }

    #[test]
    fn other_values_files_beside_a_chart_are_helm_too() {
        let tmp = tempfile::tempdir().unwrap();
        let chart = tmp.path().join("mychart");
        std::fs::create_dir_all(&chart).unwrap();
        crate::vfs::write(chart.join("Chart.yaml"), "name: mychart\n").unwrap();

        // A chart's alternate values files must get the same treatment as values.yaml.
        assert_eq!(
            detect(&chart.join("values-prod.yaml")),
            Some(Language::Helm)
        );
        // YAML elsewhere is unaffected.
        assert_eq!(
            detect(&tmp.path().join("ci/config.yaml")),
            Some(Language::Yaml)
        );
    }

    #[test]
    fn templates_dir_with_chart_yaml_is_helm() {
        let tmp = tempfile::tempdir().unwrap();
        let chart_root = tmp.path().join("mychart");
        std::fs::create_dir_all(chart_root.join("templates")).unwrap();
        crate::vfs::write(chart_root.join("Chart.yaml"), "name: mychart\n").unwrap();

        let template = chart_root.join("templates/deployment.yaml");
        assert_eq!(detect(&template), Some(Language::Helm));

        // Same layout, no Chart.yaml and no template action: plain YAML.
        let other = tmp.path().join("other/templates/thing.yaml");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        crate::vfs::write(&other, "kind: Service\n").unwrap();
        assert_eq!(detect(&other), Some(Language::Yaml));
    }

    #[test]
    fn a_templates_file_writing_actions_is_helm_without_chart_metadata() {
        // `{{ .Values.x }}` is not valid YAML.
        let tmp = tempfile::tempdir().unwrap();
        let chart = tmp.path().join("svc/chart");
        std::fs::create_dir_all(chart.join("templates")).unwrap();
        crate::vfs::write(chart.join("values.yaml"), "logLevel: info\n").unwrap();

        let template = chart.join("templates/deployment.yaml");
        crate::vfs::write(&template, "env:\n  - value: {{ .Values.logLevel }}\n").unwrap();
        assert_eq!(detect(&template), Some(Language::Helm));

        // The chart boundary is the directory holding `templates/`.
        assert_eq!(chart_root(&template), Some(chart.as_path()));

        // A brace that opens no action leaves the file as YAML.
        let plain = chart.join("templates/plain.yaml");
        crate::vfs::write(&plain, "note: \"see {{ the docs\"\n").unwrap();
        assert_eq!(detect(&plain), Some(Language::Yaml));
    }

    #[test]
    fn class_split_matches_analysis_capability() {
        assert_eq!(Language::Rust.class(), LanguageClass::Imperative);
        assert_eq!(Language::Bash.class(), LanguageClass::Imperative);
        assert_eq!(Language::Hcl.class(), LanguageClass::Config);
        assert_eq!(Language::Markdown.class(), LanguageClass::Config);
    }

    #[test]
    fn every_language_has_a_unique_name_and_roundtrips() {
        let mut seen = std::collections::HashSet::new();
        for lang in Language::ALL {
            assert!(seen.insert(lang.name()), "duplicate name: {}", lang.name());
            assert_eq!(Language::from_name(lang.name()), Some(*lang));
        }
    }

    #[test]
    fn extensions_do_not_collide_across_languages() {
        let mut owner: std::collections::HashMap<&str, Language> = Default::default();
        for lang in Language::ALL {
            for ext in lang.extensions() {
                if let Some(prev) = owner.insert(ext, *lang) {
                    panic!("extension {ext} claimed by both {prev} and {lang}");
                }
            }
        }
        assert_eq!(owner.get("tsx"), Some(&Language::Tsx));
        let _ = PathBuf::new();
    }
}
