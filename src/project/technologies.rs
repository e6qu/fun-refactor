use super::{bounded_text, hash, page, Project, RelationshipOptions};
use crate::lang::Language;
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const TECHNOLOGY_SCHEMA: &str = "fr-project-technologies-1";
const MAX_EVIDENCE_LIMIT: usize = 32;

pub fn technology_evidence_emitted(total: usize, limit: usize) -> usize {
    total.min(limit.min(MAX_EVIDENCE_LIMIT))
}

pub fn technology_evidence_omitted(total: usize, limit: usize) -> usize {
    total.saturating_sub(technology_evidence_emitted(total, limit))
}

#[derive(clap::Args)]
pub struct Options {
    #[command(flatten)]
    pub(super) selection: RelationshipOptions,
    #[arg(
        long,
        default_value_t = 4,
        help = "Evidence rows retained per technology, from 1 through 32."
    )]
    pub(super) evidence_limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Technology {
    JavaScript,
    TypeScript,
    React,
    NextJs,
    Go,
    Python,
    FastApi,
    Html,
    Css,
    TailwindCss,
    ExpressJs,
    Markdown,
    Mermaid,
}

impl Technology {
    const ALL: &'static [Self] = &[
        Self::JavaScript,
        Self::TypeScript,
        Self::React,
        Self::NextJs,
        Self::Go,
        Self::Python,
        Self::FastApi,
        Self::Html,
        Self::Css,
        Self::TailwindCss,
        Self::ExpressJs,
        Self::Markdown,
        Self::Mermaid,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::React => "react",
            Self::NextJs => "nextjs",
            Self::Go => "go",
            Self::Python => "python",
            Self::FastApi => "python-fastapi",
            Self::Html => "html",
            Self::Css => "css",
            Self::TailwindCss => "tailwind-css",
            Self::ExpressJs => "expressjs",
            Self::Markdown => "markdown",
            Self::Mermaid => "markdown-mermaid",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::React => "React",
            Self::NextJs => "Next.js",
            Self::Go => "Go",
            Self::Python => "Python",
            Self::FastApi => "Python FastAPI",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::TailwindCss => "Tailwind CSS",
            Self::ExpressJs => "Express.js",
            Self::Markdown => "Markdown",
            Self::Mermaid => "Mermaid embedded in Markdown",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::JavaScript
            | Self::TypeScript
            | Self::Go
            | Self::Python
            | Self::Html
            | Self::Css
            | Self::Markdown => "language",
            Self::React | Self::NextJs | Self::FastApi | Self::ExpressJs => "framework",
            Self::TailwindCss => "styling-framework",
            Self::Mermaid => "embedded-language",
        }
    }

    fn parser(self) -> &'static str {
        match self {
            Self::JavaScript | Self::TypeScript | Self::NextJs | Self::ExpressJs => "typescript",
            Self::React => "tsx",
            Self::Go => "go",
            Self::Python | Self::FastApi => "python",
            Self::Html => "html",
            Self::Css | Self::TailwindCss => "css-and-host-grammars",
            Self::Markdown => "markdown-block-and-inline",
            Self::Mermaid => "bounded-markdown-fence-reader",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Evidence {
    path: PathBuf,
    line: Option<usize>,
    basis: &'static str,
}

fn extension(path: &Path) -> &str {
    path.extension()
        .and_then(|part| part.to_str())
        .unwrap_or("")
}

fn npm_dependency(document: &Value, name: &str) -> bool {
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .any(|section| {
        document[*section][name]
            .as_str()
            .is_some_and(|version| !version.trim().is_empty())
    })
}

fn source_mentions_import(source: &str, module: &str) -> bool {
    let double = format!("\"{module}\"");
    let single = format!("'{module}'");
    let double_child = format!("\"{module}/");
    let single_child = format!("'{module}/");
    source.lines().any(|line| {
        let line = line.trim();
        (line.starts_with("import ") || line.starts_with("from ") || line.contains("require("))
            && (line.contains(&double)
                || line.contains(&single)
                || line.contains(&double_child)
                || line.contains(&single_child))
    })
}

fn python_mentions_import(source: &str, module: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim_start();
        line == format!("import {module}")
            || line.starts_with(&format!("import {module}."))
            || line.starts_with(&format!("import {module} as "))
            || line.starts_with(&format!("from {module} import "))
            || line.starts_with(&format!("from {module}."))
    })
}

fn mermaid_fence_lines(source: &str) -> Vec<usize> {
    let mut result = Vec::new();
    let mut open: Option<(char, usize)> = None;
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 {
            continue;
        }
        let Some(marker) = trimmed
            .chars()
            .next()
            .filter(|marker| matches!(marker, '`' | '~'))
        else {
            continue;
        };
        let count = trimmed
            .chars()
            .take_while(|character| *character == marker)
            .count();
        if count < 3 {
            continue;
        }
        if let Some((opened, width)) = open {
            if marker == opened && count >= width && trimmed[count..].trim().is_empty() {
                open = None;
            }
            continue;
        }
        let language = trimmed[count..]
            .trim()
            .split(|character: char| character.is_whitespace() || matches!(character, ',' | '{'))
            .next()
            .unwrap_or("");
        if language.eq_ignore_ascii_case("mermaid") {
            result.push(index + 1);
            open = Some((marker, count));
        }
    }
    result
}

fn push(
    rows: &mut BTreeMap<Technology, Vec<Evidence>>,
    technology: Technology,
    path: &Path,
    line: Option<usize>,
    basis: &'static str,
) {
    rows.entry(technology).or_default().push(Evidence {
        path: path.to_path_buf(),
        line,
        basis,
    });
}

impl Project<'_> {
    fn technology_file_evidence(&self, evidence: &Evidence) -> Result<Value> {
        let node = self.nodes.iter().position(|node| {
            node.symbol.is_none() && node.kind == "file" && node.path == evidence.path
        });
        let handle = node.map(|node| self.handle(node));
        let object_digest = format!(
            "frte1:{}",
            hash((
                &self.revision,
                &evidence.path,
                evidence.line,
                evidence.basis
            ))?
        );
        Ok(json!({
            "path": bounded_text(&evidence.path.to_string_lossy(), 512),
            "line": evidence.line,
            "file_handle": handle,
            "basis": evidence.basis,
            "object_digest": object_digest,
            "actions": {
                "map": ["project", "map", evidence.path.to_string_lossy()],
                "show": handle.map(|handle| vec!["project".to_owned(), "show".to_owned(), handle]),
            },
        }))
    }

    pub(super) fn technologies(&self, options: &Options) -> Result<Value> {
        if !(1..=MAX_EVIDENCE_LIMIT).contains(&options.evidence_limit) {
            anyhow::bail!("evidence-limit must be between 1 and 32");
        }
        let selected = self.relationship_selection(&options.selection)?;
        let mut found: BTreeMap<Technology, Vec<Evidence>> = BTreeMap::new();
        for (path, info) in self.index.files() {
            if !self.scope_file(selected, path) {
                continue;
            }
            let relative = path.strip_prefix(&self.root)?;
            let source = &self.sources[path];
            match (info.language, extension(relative)) {
                (Language::TypeScript, "js" | "mjs" | "cjs") => push(
                    &mut found,
                    Technology::JavaScript,
                    relative,
                    None,
                    "javascript-extension",
                ),
                (Language::TypeScript, "ts" | "mts" | "cts") => push(
                    &mut found,
                    Technology::TypeScript,
                    relative,
                    None,
                    "typescript-extension",
                ),
                (Language::Tsx, "jsx") => {
                    push(
                        &mut found,
                        Technology::JavaScript,
                        relative,
                        None,
                        "jsx-javascript-extension",
                    );
                }
                (Language::Tsx, "tsx") => {
                    push(
                        &mut found,
                        Technology::TypeScript,
                        relative,
                        None,
                        "tsx-typescript-extension",
                    );
                }
                (Language::Go, _) => push(&mut found, Technology::Go, relative, None, "go-parser"),
                (Language::Python, _) => push(
                    &mut found,
                    Technology::Python,
                    relative,
                    None,
                    "python-parser",
                ),
                (Language::Html, _) => {
                    push(&mut found, Technology::Html, relative, None, "html-parser")
                }
                (Language::Css, _) => {
                    push(&mut found, Technology::Css, relative, None, "css-parser")
                }
                (Language::Markdown, _) => {
                    push(
                        &mut found,
                        Technology::Markdown,
                        relative,
                        None,
                        "markdown-parser",
                    );
                    for line in mermaid_fence_lines(source) {
                        push(
                            &mut found,
                            Technology::Mermaid,
                            relative,
                            Some(line),
                            "markdown-mermaid-fence",
                        );
                    }
                }
                _ => {}
            }
            if matches!(info.language, Language::TypeScript | Language::Tsx) {
                if source_mentions_import(source, "react") {
                    push(
                        &mut found,
                        Technology::React,
                        relative,
                        None,
                        "react-import",
                    );
                }
                if source_mentions_import(source, "next")
                    || relative.starts_with("app")
                    || relative.starts_with("src/app")
                {
                    push(
                        &mut found,
                        Technology::NextJs,
                        relative,
                        None,
                        "nextjs-source-or-layout",
                    );
                }
                if source_mentions_import(source, "express") {
                    push(
                        &mut found,
                        Technology::ExpressJs,
                        relative,
                        None,
                        "express-import",
                    );
                }
            }
            if info.language == Language::Python && python_mentions_import(source, "fastapi") {
                push(
                    &mut found,
                    Technology::FastApi,
                    relative,
                    None,
                    "fastapi-import",
                );
            }
            let file_name = relative
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if file_name.starts_with("tailwind.config.")
                || source.contains("@tailwind ")
                || source.contains("@import \"tailwindcss\"")
                || source.contains("@import 'tailwindcss'")
            {
                push(
                    &mut found,
                    Technology::TailwindCss,
                    relative,
                    None,
                    "tailwind-config-or-directive",
                );
            }
        }
        for (manifest, document) in &self.manifests.documents {
            if !self.scope_file(selected, &self.root.join(manifest)) {
                continue;
            }
            for (technology, package) in [
                (Technology::React, "react"),
                (Technology::NextJs, "next"),
                (Technology::TailwindCss, "tailwindcss"),
                (Technology::ExpressJs, "express"),
            ] {
                if npm_dependency(document, package) {
                    push(&mut found, technology, manifest, None, "npm-dependency");
                }
            }
        }

        let mut rows = Vec::new();
        for technology in Technology::ALL {
            let evidence = found.entry(*technology).or_default();
            evidence.sort();
            evidence.dedup();
            let total = evidence.len();
            let emitted = technology_evidence_emitted(total, options.evidence_limit);
            let omitted = technology_evidence_omitted(total, options.evidence_limit);
            let disclosed = evidence
                .iter()
                .take(emitted)
                .map(|item| self.technology_file_evidence(item))
                .collect::<Result<Vec<_>>>()?;
            rows.push(json!({
                "id": technology.id(),
                "label": technology.label(),
                "kind": technology.kind(),
                "parser": technology.parser(),
                "status": if total == 0 { "absent" } else { "detected" },
                "evidence": disclosed,
                "evidence_count": total,
                "evidence_omitted": omitted,
                "evidence_limit": options.evidence_limit,
            }));
        }
        let key = format!(
            "frpt1:{}",
            &hash((
                &self.revision,
                "technologies",
                selected,
                options.evidence_limit
            ))?[..32]
        );
        let (start, end, page) = page(
            rows.len(),
            options.selection.limit,
            options.selection.cursor.as_deref(),
            &key,
        )?;
        let mut report = self.envelope("technologies");
        report["technology_schema"] = json!(TECHNOLOGY_SCHEMA);
        report["items"] = json!(&rows[start..end]);
        report["page"] = page;
        report["analysis"] = json!({
            "surface_count": Technology::ALL.len(),
            "detected": rows.iter().filter(|row| row["status"] == "detected").count(),
            "absent": rows.iter().filter(|row| row["status"] == "absent").count(),
            "evidence_limit_per_surface": options.evidence_limit,
            "scope": "Captured extensions, syntax hosts, imports, manifests, layout and Markdown fences.",
            "limitations": [
                "A detected framework is structural evidence, not proof that runtime configuration selects it.",
                "Tailwind utility semantics and Mermaid graph contents require their dedicated high-level readers.",
            ],
        });
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        mermaid_fence_lines, python_mentions_import, source_mentions_import,
        technology_evidence_emitted, technology_evidence_omitted,
    };

    #[test]
    fn mermaid_fences_require_matching_bounded_markers() {
        let source = "```mermaid\ngraph TD\n```\n\n~~~~ Mermaid {theme=dark}\nA-->B\n~~~~\n";
        assert_eq!(mermaid_fence_lines(source), vec![1, 5]);
        assert!(mermaid_fence_lines("    ```mermaid\nA-->B\n    ```\n").is_empty());
        assert!(mermaid_fence_lines("```rust\n// mermaid\n```\n").is_empty());
    }

    #[test]
    fn framework_import_evidence_covers_subpaths_and_aliases() {
        assert!(source_mentions_import(
            "import { NextRequest } from 'next/server';",
            "next"
        ));
        assert!(source_mentions_import(
            "const app = require(\"express\");",
            "express"
        ));
        assert!(!source_mentions_import("import x from 'nextish';", "next"));
        assert!(python_mentions_import("import fastapi as api", "fastapi"));
        assert!(python_mentions_import(
            "from fastapi.security import OAuth2",
            "fastapi"
        ));
        assert!(!python_mentions_import("import fastapi_tools", "fastapi"));
    }

    #[test]
    fn evidence_partition_obeys_the_public_limit_and_hard_ceiling() {
        assert_eq!(technology_evidence_emitted(40, 4), 4);
        assert_eq!(technology_evidence_omitted(40, 4), 36);
        assert_eq!(technology_evidence_emitted(40, usize::MAX), 32);
        assert_eq!(technology_evidence_omitted(40, usize::MAX), 8);
    }
}
