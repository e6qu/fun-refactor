//! Language identity and dialect detection.
//!
//! Adapted from funveil's `Language` enum, with dialects split apart where a
//! refactoring tool must treat them differently:
//!
//! - `TypeScript` vs `Tsx` — genuinely different tree-sitter grammars.
//! - `Css` vs `Scss` — SCSS adds `$variables`, nesting and `@mixin`; the plain CSS
//!   grammar reports them as errors, so the dialect must be visible to callers.
//! - `Yaml` vs `Helm` — Helm templates are YAML with Go template actions that are not
//!   valid YAML at all.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// A source language, at the granularity that matters for parsing and refactoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
    Hcl,
    Yaml,
    Helm,
    Xml,
    Markdown,
}

/// Broad language class. Determines which analyses even apply: imperative languages
/// get call graphs and dataflow, config/markup languages get provenance instead.
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
        Language::Hcl,
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
            Language::Hcl => "hcl",
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
            | Language::Hcl
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
            Language::TypeScript => &["ts", "mts", "cts"],
            Language::Tsx => &["tsx"],
            Language::Python => &["py", "pyi"],
            Language::Bash => &["sh", "bash"],
            Language::Html => &["html", "htm"],
            Language::Css => &["css"],
            Language::Scss => &["scss", "sass"],
            Language::Hcl => &["tf", "tfvars", "hcl"],
            Language::Yaml => &["yaml", "yml"],
            // Helm shares YAML extensions and is otherwise distinguished by chart
            // layout, but `.tpl` is unambiguously a Helm template file.
            Language::Helm => &["tpl"],
            Language::Xml => &["xml"],
            Language::Markdown => &["md", "markdown", "mdown", "mkd"],
        }
    }

    /// Does a name resolve across every file in the same directory?
    ///
    /// Terraform's unit of scope is the module, which *is* a directory: a `var.x`
    /// written in `main.tf` refers to the `variable "x"` block wherever in that
    /// directory it is declared. Without modelling that, a rename would update the
    /// declaration and leave every use behind.
    pub fn resolves_by_directory(&self) -> bool {
        matches!(self, Language::Hcl)
    }

    /// Is every member of a value reached through a receiver written before it?
    ///
    /// Where this holds, a call with no receiver cannot be a method, which is what
    /// stops a bare `contextWithTimeout(…)` from resolving to the `statusWaiter`
    /// method of that name sitting four lines above it. Rust is excluded: it reaches
    /// an associated function through a path, `Foo::new()`, which is not a receiver
    /// and is not recorded as one.
    pub fn members_always_have_a_receiver(&self) -> bool {
        matches!(
            self,
            Language::Go
                | Language::TypeScript
                | Language::Tsx
                | Language::Python
                | Language::Zig
                | Language::Java
        )
    }

    /// Is a package here a directory, so that top-level declarations are visible
    /// unqualified from every file beside them?
    ///
    /// Go's package is the directory. A function in `a.go` is called from `b.go` with
    /// no import and no qualifier, which is why resolution cannot stop at the file.
    /// This is narrower than [`Self::resolves_by_directory`]: only *top-level*
    /// declarations are in package scope, so methods and struct fields are not.
    pub fn packages_by_directory(&self) -> bool {
        matches!(self, Language::Go)
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
        f.write_str(self.name())
    }
}

/// Detect a file's language from its path.
///
/// YAML files are classified as Helm when the chart layout says so: a `templates/`
/// directory ancestor, or one of the well-known chart files. Helm templates are
/// parsed differently because `{{ ... }}` actions are not valid YAML.
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

    // Any YAML sitting beside a Chart.yaml belongs to that chart — charts routinely
    // carry values-prod.yaml and friends, and treating them as plain YAML would give
    // them different provenance rules than the values.yaml next to them.
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
}

/// The chart directory a Helm file belongs to: the nearest ancestor with a Chart.yaml.
///
/// A chart's values are its own. Two charts in one workspace routinely declare the
/// same key — `image`, `name`, `replicaCount` — and a `.Values.image` in one of them
/// says nothing about the other. Resolution needs the boundary to avoid pointing a
/// template at a neighbour's values file.
pub fn chart_root(path: &Path) -> Option<&Path> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if crate::vfs::exists(d.join("Chart.yaml")) || crate::vfs::exists(d.join("chart.yaml")) {
            return Some(d);
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
///
/// Resolution matches candidates by name across the whole workspace, and until this
/// existed it did so without asking what language they were written in. A Rust
/// `out.push(…)` therefore resolved to a Zig `Ring.push` — at `import-qualified`,
/// a tier the tool *rewrites* — so renaming the Zig method silently turned a
/// `Vec::push` call in Rust into `out.pushReading(…)`. Two files, two languages, no
/// relationship whatsoever, and a diff that looked ordinary.
///
/// A cross-language edge is only real where the two languages have a mechanism for
/// naming each other's declarations. Those mechanisms are enumerated here rather than
/// inferred, because the cost of a wrong one is an edit that compiles somewhere else
/// and breaks here.
///
/// What is deliberately absent: any pair of imperative languages. Rust cannot name a
/// Zig method, Go cannot name a Python function, and where an FFI does connect them
/// the binding is declared in a build file this tool does not read. Reporting those
/// as unresolved is the honest answer.
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
        (Css, Scss) | (Scss, Css) => true,

        // Markup names a style rule by class or id. This is the edge that makes
        // renaming a CSS class across an HTML template worth having.
        (Html | Xml | Tsx | TypeScript | Markdown, Css | Scss) => {
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

        // Same layout without a Chart.yaml is just YAML.
        let other = tmp.path().join("other/templates/thing.yaml");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        assert_eq!(detect(&other), Some(Language::Yaml));
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
