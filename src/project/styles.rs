use super::{bounded_text, hash, page, Project, RelationshipOptions};
use crate::lang::Language;
use crate::surface_kernel;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const STYLE_SCHEMA: &str = "fr-project-styles-1";

#[derive(Clone)]
struct LocatedName {
    path: PathBuf,
    line: usize,
    name: String,
}

#[derive(Clone)]
enum ClassAttribute {
    Literal {
        line: usize,
        names: Vec<String>,
        omitted: usize,
    },
    Dynamic {
        line: usize,
    },
}

fn css_classes(source: &str, path: &Path) -> Vec<LocatedName> {
    let mut result = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let Some((selector, _)) = line.split_once('{') else {
            continue;
        };
        let bytes = selector.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != b'.'
                || at > 0
                    && (bytes[at - 1].is_ascii_alphanumeric()
                        || matches!(bytes[at - 1], b'_' | b'-' | b'\\'))
            {
                at += 1;
                continue;
            }
            let mut cursor = at + 1;
            let mut name = String::new();
            while cursor < bytes.len() {
                if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                    name.push(bytes[cursor + 1] as char);
                    cursor += 2;
                } else if bytes[cursor].is_ascii_alphanumeric()
                    || matches!(bytes[cursor], b'_' | b'-' | b'/')
                {
                    name.push(bytes[cursor] as char);
                    cursor += 1;
                } else {
                    break;
                }
            }
            if !name.is_empty() {
                result.push(LocatedName {
                    path: path.to_path_buf(),
                    line: index + 1,
                    name,
                });
            }
            at = cursor.max(at + 1);
        }
    }
    result
}

fn class_attributes(source: &str) -> Vec<ClassAttribute> {
    let mut result = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let mut rest = line;
        loop {
            let class = rest.find("class").map(|at| (at, 5));
            let class_name = rest.find("className").map(|at| (at, 9));
            let Some((at, width)) = [class_name, class]
                .into_iter()
                .flatten()
                .min_by_key(|(at, width)| (*at, usize::MAX - *width))
            else {
                break;
            };
            let prefix = &rest[..at];
            let in_tag = prefix.rfind('<') > prefix.rfind('>');
            let trimmed_line = line.trim_start();
            let attribute_continuation = prefix.trim().is_empty()
                && ["class=", "class =", "className=", "className ="]
                    .iter()
                    .any(|start| trimmed_line.starts_with(start));
            if !in_tag && !attribute_continuation {
                rest = &rest[at + width..];
                continue;
            }
            if at > 0 && rest.as_bytes()[at - 1].is_ascii_alphanumeric() {
                rest = &rest[at + width..];
                continue;
            }
            let tail = &rest[at + width..];
            if tail
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                rest = tail;
                continue;
            }
            let Some(value) = tail.trim_start().strip_prefix('=') else {
                rest = tail;
                continue;
            };
            let value = value.trim_start();
            let (value, expression) = if let Some(value) = value.strip_prefix('{') {
                (value.trim_start(), true)
            } else {
                (value, false)
            };
            let quoted = value
                .as_bytes()
                .first()
                .copied()
                .filter(|quote| matches!(quote, b'\'' | b'"' | b'`'));
            if let Some(quote) = quoted {
                let tail = &value[1..];
                if let Some(end) = tail.as_bytes().iter().position(|byte| *byte == quote) {
                    let literal = &tail[..end];
                    if quote != b'`' || !literal.contains("${") {
                        let all = literal
                            .split_ascii_whitespace()
                            .filter(|name| !name.is_empty())
                            .collect::<Vec<_>>();
                        let names = all
                            .iter()
                            .take(128)
                            .map(|name| (*name).to_owned())
                            .collect();
                        result.push(ClassAttribute::Literal {
                            line: index + 1,
                            names,
                            omitted: all.len().saturating_sub(128),
                        });
                        rest = &tail[end + 1..];
                        if expression {
                            rest = rest.trim_start().strip_prefix('}').unwrap_or(rest);
                        }
                        continue;
                    }
                }
            }
            result.push(ClassAttribute::Dynamic { line: index + 1 });
            rest = &rest[at + width..];
        }
    }
    result
}

fn npm_dependency(document: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies"].iter().any(|section| {
        document[*section][name]
            .as_str()
            .is_some_and(|version| !version.trim().is_empty())
    })
}

impl Project<'_> {
    fn tailwind_manifest(&self, relative: &Path) -> Option<PathBuf> {
        relative
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .map(|directory| directory.join("package.json"))
            .find(|manifest| {
                self.manifests
                    .documents
                    .get(manifest)
                    .is_some_and(|document| npm_dependency(document, "tailwindcss"))
            })
    }

    pub(super) fn styles(&self, options: &RelationshipOptions) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        let mut definitions = Vec::new();
        let mut host_attributes = Vec::new();
        let mut files = BTreeSet::new();
        let mut tailwind_manifests = BTreeSet::new();
        for (file, info) in self.index.files() {
            if !self.scope_file(selected, file) {
                continue;
            }
            let relative = file.strip_prefix(&self.root)?;
            let source = &self.sources[file];
            match info.language {
                Language::Css => {
                    files.insert(relative.to_path_buf());
                    definitions.extend(css_classes(source, relative));
                    if source.lines().any(|line| {
                        let line = line.trim_start();
                        line.starts_with("@tailwind ")
                            || line.starts_with("@import \"tailwindcss\"")
                    }) {
                        if let Some(manifest) = self.tailwind_manifest(relative) {
                            tailwind_manifests.insert(manifest);
                        }
                    }
                }
                Language::Html | Language::Tsx => {
                    let attributes = class_attributes(source);
                    if !attributes.is_empty() {
                        files.insert(relative.to_path_buf());
                        host_attributes.push((relative.to_path_buf(), attributes));
                    }
                    if let Some(manifest) = self.tailwind_manifest(relative) {
                        tailwind_manifests.insert(manifest);
                    }
                }
                _ => {}
            }
        }

        let mut rows = Vec::new();
        let mut file_ids = BTreeMap::new();
        for path in files {
            let id = format!("frsf1:{}", &hash((&self.revision, &path))?[..32]);
            file_ids.insert(path.clone(), id.clone());
            rows.push(json!({
                "kind": "style-file", "id": id, "parent": null,
                "source": self.file_source(&path, 1),
                "status": "observed", "confidence": "exact",
                "evidence": {"basis": "captured-style-host", "validation": ["captured-source", "project-revision"]},
                "gaps": [],
                "style_file": {"path": bounded_text(&path.to_string_lossy(), 512)},
            }));
        }
        let mut definition_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for definition in &definitions {
            let detail = json!([definition.path, definition.line, definition.name]);
            let id = format!("frsd1:{}", &hash((&self.revision, &detail))?[..32]);
            definition_ids
                .entry(definition.name.clone())
                .or_default()
                .push(id.clone());
            rows.push(json!({
                "kind": "css-class-definition", "id": id,
                "parent": file_ids[&definition.path],
                "source": self.file_source(&definition.path, definition.line),
                "status": "observed", "confidence": "syntax-only",
                "evidence": {"basis": "css-class-selector", "validation": ["captured-source", "bounded-selector-reader", "project-revision"]},
                "gaps": ["The selector reader does not evaluate nesting, cascade, specificity or generated CSS."],
                "class": {"name": bounded_text(&definition.name, 256)},
            }));
        }
        let mut literal_uses = 0usize;
        let mut literal_uses_omitted = 0usize;
        let mut dynamic_gaps = 0usize;
        for (path, attributes) in host_attributes {
            let tailwind = self.tailwind_manifest(&path).is_some();
            for attribute in attributes {
                match attribute {
                    ClassAttribute::Literal {
                        line,
                        names,
                        omitted,
                    } => {
                        literal_uses_omitted += omitted;
                        literal_uses += omitted;
                        for name in names {
                            literal_uses += 1;
                            let targets = definition_ids.get(&name).cloned().unwrap_or_default();
                            let (status, confidence, basis, gaps) = match surface_kernel::style_literal_resolution(targets.len(), tailwind) {
                                0 => ("resolved", "name-only", "literal-css-class-use", json!(["Name agreement does not prove selector applicability or final cascade."])),
                                1 => ("ambiguous", "name-only", "literal-css-class-use", json!(["Multiple captured CSS definitions share this class name."])),
                                2 => ("candidate", "syntax-only", "tailwind-literal-utility-candidate", json!(["The nearest package declares Tailwind CSS; utility availability, variants and generated output remain unchecked."])),
                                _ => ("unresolved", "unknown", "literal-class-use", json!(["No captured CSS definition or Tailwind package evidence resolves this literal class."])),
                            };
                            let detail = json!([path, line, name]);
                            rows.push(json!({
                                "kind": "class-use", "id": format!("frsu1:{}", &hash((&self.revision, &detail))?[..32]),
                                "parent": file_ids[&path], "source": self.file_source(&path, line),
                                "status": status, "confidence": confidence,
                                "evidence": {"basis": basis, "validation": ["captured-source", "bounded-attribute-reader", "project-revision"]},
                                "gaps": gaps,
                                "class_use": {"name": bounded_text(&name, 256), "definition_ids": targets,
                                    "tailwind_context": tailwind},
                            }));
                        }
                        if omitted > 0 {
                            let reason =
                                "The per-attribute class-token limit omitted literal tokens.";
                            rows.push(json!({
                                "kind": "style-gap", "id": format!("frsg1:{}", &hash((&self.revision, &path, line, reason))?[..32]),
                                "parent": file_ids[&path], "source": self.file_source(&path, line),
                                "status": "gap", "confidence": null,
                                "evidence": {"basis": "class-token-limit", "validation": ["captured-source", "bounded-attribute-reader", "project-revision"]},
                                "gaps": [reason], "gap": {"reason": reason, "omitted": omitted},
                            }));
                        }
                    }
                    ClassAttribute::Dynamic { line } => {
                        dynamic_gaps += 1;
                        let reason = "A class or className attribute is not one supported direct quoted literal.";
                        rows.push(json!({
                            "kind": "style-gap", "id": format!("frsg1:{}", &hash((&self.revision, &path, line, reason))?[..32]),
                            "parent": file_ids[&path], "source": self.file_source(&path, line),
                            "status": "gap", "confidence": null,
                            "evidence": {"basis": "dynamic-class-attribute", "validation": ["captured-source", "bounded-attribute-reader", "project-revision"]},
                            "gaps": [reason], "gap": {"reason": reason},
                        }));
                    }
                }
            }
        }
        for manifest in &tailwind_manifests {
            let id = format!("frstw1:{}", &hash((&self.revision, manifest))?[..32]);
            rows.push(json!({
                "kind": "tailwind-context", "id": id, "parent": null,
                "source": self.file_source(manifest, 1), "status": "candidate", "confidence": "manifest-only",
                "evidence": {"basis": "npm-tailwind-dependency", "validation": ["captured-manifest", "project-revision"]},
                "gaps": ["Dependency evidence does not prove the active Tailwind configuration, generated utilities or build integration."],
                "tailwind": {"manifest": bounded_text(&manifest.to_string_lossy(), 512)},
            }));
        }
        rows.sort_by_cached_key(Value::to_string);
        let key = format!(
            "frpsc1:{}",
            &hash((&self.revision, "styles", selected))?[..32]
        );
        let (start, end, page) = page(rows.len(), options.limit, options.cursor.as_deref(), &key)?;
        let mut report = self.envelope("styles");
        report["style_schema"] = json!(STYLE_SCHEMA);
        report["items"] = json!(&rows[start..end]);
        report["page"] = page;
        report["analysis"] = json!({
            "css_class_definitions": definitions.len(), "literal_class_uses": literal_uses,
            "literal_class_uses_omitted": literal_uses_omitted,
            "dynamic_class_gaps": dynamic_gaps, "tailwind_contexts": tailwind_manifests.len(),
            "certainty": "Class definitions and direct literal host tokens preserve syntax evidence; relationships do not model the cascade or generated output.",
        });
        Ok(report)
    }
}
