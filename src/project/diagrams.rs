use super::{bounded_text, hash, page, Project, RelationshipOptions};
use crate::lang::Language;
use crate::surface_kernel;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const DIAGRAM_SCHEMA: &str = "fr-project-diagrams-1";
const MERMAID_NODE_LIMIT: usize = 512;
const MERMAID_EDGE_LIMIT: usize = 1024;

struct Heading {
    level: usize,
    line: usize,
    title: String,
}

struct Fence {
    line: usize,
    end_line: Option<usize>,
    body: Vec<(usize, String)>,
}

fn fence_start(line: &str) -> Option<(char, usize, bool)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed
        .chars()
        .next()
        .filter(|value| matches!(value, '`' | '~'))?;
    let width = trimmed.chars().take_while(|value| *value == marker).count();
    if width < 3 {
        return None;
    }
    let language = trimmed[width..]
        .trim()
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | '{'))
        .next()
        .unwrap_or("");
    Some((marker, width, language.eq_ignore_ascii_case("mermaid")))
}

fn markdown_structure(source: &str) -> (Vec<Heading>, Vec<Fence>) {
    let lines = source.lines().collect::<Vec<_>>();
    let mut headings = Vec::new();
    let mut fences = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if let Some((marker, width, mermaid)) = fence_start(line) {
            let start = at + 1;
            at += 1;
            let mut body = Vec::new();
            let mut end = None;
            while at < lines.len() {
                let trimmed = lines[at].trim_start_matches(' ');
                let closing = trimmed.chars().take_while(|value| *value == marker).count();
                if closing >= width && trimmed[closing..].trim().is_empty() {
                    end = Some(at + 1);
                    break;
                }
                if mermaid {
                    body.push((at + 1, lines[at].to_owned()));
                }
                at += 1;
            }
            if mermaid {
                fences.push(Fence {
                    line: start,
                    end_line: end,
                    body,
                });
            }
        } else {
            let trimmed = line.trim_start();
            let level = trimmed.chars().take_while(|value| *value == '#').count();
            if (1..=6).contains(&level)
                && trimmed
                    .as_bytes()
                    .get(level)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                headings.push(Heading {
                    level,
                    line: at + 1,
                    title: trimmed[level..].trim().to_owned(),
                });
            }
        }
        at += 1;
    }
    (headings, fences)
}

fn diagram_kind(body: &[(usize, String)]) -> &'static str {
    let first = body
        .iter()
        .map(|(_, line)| line.trim())
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    let keyword = first.split_ascii_whitespace().next().unwrap_or("");
    match keyword {
        "flowchart" | "graph" => "flowchart",
        "sequenceDiagram" => "sequence",
        "classDiagram" => "class",
        "stateDiagram" | "stateDiagram-v2" => "state",
        "erDiagram" => "entity-relationship",
        "journey" => "journey",
        "gantt" => "gantt",
        "pie" => "pie",
        "gitGraph" => "git-graph",
        "mindmap" => "mindmap",
        "timeline" => "timeline",
        "quadrantChart" => "quadrant",
        "xychart-beta" => "xy-chart",
        "block-beta" => "block",
        "packet-beta" => "packet",
        "architecture-beta" => "architecture",
        _ => "unknown",
    }
}

fn endpoint(text: &str) -> Option<String> {
    let text = text.trim().trim_matches('|').trim();
    let text = text.split(':').next().unwrap_or(text).trim();
    let text = text.split_ascii_whitespace().next().unwrap_or("");
    let end = text
        .char_indices()
        .find(|(_, character)| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '-' | '.')
        })
        .map_or(text.len(), |(at, _)| at);
    let name = &text[..end];
    (!name.is_empty()).then(|| name.to_owned())
}

fn edge(line: &str) -> Option<(String, String, &'static str)> {
    for (operator, kind) in [
        ("-->>", "async-message"),
        ("-.->", "dotted-arrow"),
        ("-->", "arrow"),
        ("==>", "thick-arrow"),
        ("->>", "message"),
        ("---", "line"),
        ("-x", "cross-message"),
        ("-)", "open-message"),
    ] {
        if let Some((left, right)) = line.split_once(operator) {
            return Some((endpoint(left)?, endpoint(right)?, kind));
        }
    }
    None
}

fn declared_node(line: &str) -> Option<String> {
    let line = line.trim();
    for prefix in ["participant ", "actor "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return endpoint(rest);
        }
    }
    let candidate = endpoint(line)?;
    line[candidate.len()..]
        .trim_start()
        .starts_with(['[', '(', '{', '>'])
        .then_some(candidate)
}

fn directive(line: &str) -> bool {
    let line = line.trim();
    line.is_empty()
        || line.starts_with("%%")
        || [
            "flowchart",
            "graph",
            "sequenceDiagram",
            "classDiagram",
            "stateDiagram",
            "stateDiagram-v2",
            "erDiagram",
            "journey",
            "gantt",
            "pie",
            "gitGraph",
            "mindmap",
            "timeline",
            "quadrantChart",
            "xychart-beta",
            "block-beta",
            "packet-beta",
            "architecture-beta",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

fn mermaid_node_id(revision: &str, diagram: &str, name: &str) -> Result<String> {
    Ok(format!("frmn1:{}", &hash((revision, diagram, name))?[..32]))
}

impl Project<'_> {
    pub(super) fn diagrams(&self, options: &RelationshipOptions) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        let mut rows = Vec::new();
        let mut document_count = 0usize;
        let mut heading_count = 0usize;
        let mut diagram_count = 0usize;
        let mut node_count = 0usize;
        let mut edge_count = 0usize;
        let mut gap_count = 0usize;
        let mut nodes_omitted = 0usize;
        let mut edges_omitted = 0usize;
        for (file, info) in self.index.files() {
            if info.language != Language::Markdown || !self.scope_file(selected, file) {
                continue;
            }
            document_count += 1;
            let relative = file.strip_prefix(&self.root)?;
            let source = &self.sources[file];
            let (headings, fences) = markdown_structure(source);
            let document_id = format!("frmd1:{}", &hash((&self.revision, relative))?[..32]);
            rows.push(json!({
                "kind": "markdown-document", "id": document_id, "parent": null,
                "source": self.file_source(relative, 1), "status": "observed", "confidence": "exact",
                "evidence": {"basis": "captured-markdown-document", "validation": ["captured-source", "project-revision"]},
                "gaps": [], "document": {"path": bounded_text(&relative.to_string_lossy(), 512),
                    "heading_count": headings.len(), "mermaid_count": fences.len()},
            }));
            let mut heading_stack: Vec<(usize, String)> = Vec::new();
            let mut heading_ids = Vec::new();
            for heading in &headings {
                while heading_stack
                    .last()
                    .is_some_and(|(level, _)| *level >= heading.level)
                {
                    heading_stack.pop();
                }
                let parent = heading_stack
                    .last()
                    .map(|(_, id)| id.clone())
                    .unwrap_or_else(|| document_id.clone());
                let id = format!(
                    "frmh1:{}",
                    &hash((
                        &self.revision,
                        relative,
                        heading.line,
                        heading.level,
                        &heading.title
                    ))?[..32]
                );
                rows.push(json!({
                    "kind": "markdown-heading", "id": id, "parent": parent,
                    "source": self.file_source(relative, heading.line), "status": "observed", "confidence": "syntax-only",
                    "evidence": {"basis": "markdown-atx-heading", "validation": ["captured-source", "bounded-markdown-reader", "project-revision"]},
                    "gaps": [], "heading": {"level": heading.level, "title": bounded_text(&heading.title, 256)},
                }));
                heading_count += 1;
                heading_stack.push((heading.level, id.clone()));
                heading_ids.push((heading.line, id));
            }
            for fence in fences {
                diagram_count += 1;
                let parent = heading_ids
                    .iter()
                    .rev()
                    .find(|(line, _)| *line < fence.line)
                    .map(|(_, id)| id.clone())
                    .unwrap_or_else(|| document_id.clone());
                let kind = diagram_kind(&fence.body);
                let diagram_id = format!(
                    "frmg1:{}",
                    &hash((&self.revision, relative, fence.line, fence.end_line, kind))?[..32]
                );
                rows.push(json!({
                    "kind": "mermaid-diagram", "id": diagram_id, "parent": parent,
                    "source": self.file_source(relative, fence.line),
                    "status": if fence.end_line.is_some() && kind != "unknown" { "candidate" } else { "gap" },
                    "confidence": if kind == "unknown" { Value::Null } else { json!("syntax-only") },
                    "evidence": {"basis": "markdown-mermaid-fence", "validation": ["captured-source", "project-revision"]},
                    "gaps": if fence.end_line.is_none() { json!(["The Mermaid fence has no captured closing marker."])
                        } else if kind == "unknown" { json!(["The Mermaid diagram kind is not recognized."])
                        } else { json!(["Graph extraction does not execute Mermaid or validate renderer-specific semantics."]) },
                    "diagram": {"diagram_kind": kind, "opening_line": fence.line, "closing_line": fence.end_line},
                }));
                let mut nodes: BTreeMap<String, (String, usize)> = BTreeMap::new();
                let mut edges = Vec::new();
                let mut gaps = Vec::new();
                for (line_number, source_line) in &fence.body {
                    let text = source_line.trim();
                    if let Some((from, to, edge_kind)) = edge(text) {
                        if !nodes.contains_key(&from) {
                            nodes.insert(
                                from.clone(),
                                (
                                    mermaid_node_id(&self.revision, &diagram_id, &from)?,
                                    *line_number,
                                ),
                            );
                        }
                        if !nodes.contains_key(&to) {
                            nodes.insert(
                                to.clone(),
                                (
                                    mermaid_node_id(&self.revision, &diagram_id, &to)?,
                                    *line_number,
                                ),
                            );
                        }
                        edges.push((*line_number, from, to, edge_kind));
                    } else if let Some(name) = declared_node(text) {
                        if !nodes.contains_key(&name) {
                            nodes.insert(
                                name.clone(),
                                (
                                    mermaid_node_id(&self.revision, &diagram_id, &name)?,
                                    *line_number,
                                ),
                            );
                        }
                    } else if !directive(text) {
                        gaps.push(*line_number);
                    }
                }
                let node_emitted =
                    surface_kernel::surface_items_emitted(nodes.len(), MERMAID_NODE_LIMIT);
                nodes_omitted +=
                    surface_kernel::surface_items_omitted(nodes.len(), MERMAID_NODE_LIMIT);
                let retained_nodes = nodes
                    .keys()
                    .take(node_emitted)
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                for (name, (id, line)) in nodes
                    .iter()
                    .filter(|(name, _)| retained_nodes.contains(*name))
                {
                    node_count += 1;
                    rows.push(json!({
                        "kind": "mermaid-node", "id": id, "parent": diagram_id,
                        "source": self.file_source(relative, *line), "status": "candidate", "confidence": "name-only",
                        "evidence": {"basis": "mermaid-node-identifier", "validation": ["captured-source", "project-revision"]},
                        "gaps": ["Identifier extraction does not preserve or validate the rendered label and shape."],
                        "node": {"name": bounded_text(name, 256)},
                    }));
                }
                let total_edges = edges.len();
                let retained_edges = edges
                    .into_iter()
                    .filter(|(_, from, to, _)| {
                        retained_nodes.contains(from) && retained_nodes.contains(to)
                    })
                    .collect::<Vec<_>>();
                let edge_emitted =
                    surface_kernel::surface_items_emitted(retained_edges.len(), MERMAID_EDGE_LIMIT);
                edges_omitted += total_edges.saturating_sub(edge_emitted);
                for (line, from, to, edge_kind) in retained_edges.into_iter().take(edge_emitted) {
                    edge_count += 1;
                    let detail = json!([line, from, to, edge_kind]);
                    rows.push(json!({
                        "kind": "mermaid-edge", "id": format!("frme1:{}", &hash((&self.revision, &diagram_id, &detail))?[..32]),
                        "parent": diagram_id, "source": self.file_source(relative, line),
                        "status": "candidate", "confidence": "name-only",
                        "evidence": {"basis": "mermaid-edge-operator", "validation": ["captured-source", "project-revision"]},
                        "gaps": ["Edge extraction does not validate the complete diagram grammar or rendered meaning."],
                        "edge": {"from": from, "to": to, "edge_kind": edge_kind,
                            "from_id": nodes[&from].0, "to_id": nodes[&to].0},
                    }));
                }
                for line in gaps {
                    gap_count += 1;
                    let reason =
                        "The Mermaid line is outside the bounded node, edge and directive reader.";
                    rows.push(json!({
                        "kind": "diagram-gap", "id": format!("frmx1:{}", &hash((&self.revision, &diagram_id, line, reason))?[..32]),
                        "parent": diagram_id, "source": self.file_source(relative, line),
                        "status": "gap", "confidence": null,
                        "evidence": {"basis": "unmodeled-mermaid-line", "validation": ["captured-source", "project-revision"]},
                        "gaps": [reason], "gap": {"reason": reason},
                    }));
                }
                let omitted_nodes =
                    surface_kernel::surface_items_omitted(nodes.len(), MERMAID_NODE_LIMIT);
                let omitted_edges = total_edges.saturating_sub(edge_emitted);
                if omitted_nodes > 0 || omitted_edges > 0 {
                    gap_count += 1;
                    let reason = "The per-diagram graph limits omitted Mermaid nodes or edges.";
                    rows.push(json!({
                        "kind": "diagram-gap", "id": format!("frmx1:{}", &hash((&self.revision, &diagram_id, reason))?[..32]),
                        "parent": diagram_id, "source": self.file_source(relative, fence.line),
                        "status": "gap", "confidence": null,
                        "evidence": {"basis": "mermaid-graph-limit", "validation": ["captured-source", "project-revision"]},
                        "gaps": [reason], "gap": {"reason": reason,
                            "nodes_omitted": omitted_nodes, "edges_omitted": omitted_edges},
                    }));
                }
            }
        }
        rows.sort_by_cached_key(Value::to_string);
        let key = format!(
            "frpdc1:{}",
            &hash((&self.revision, "diagrams", selected))?[..32]
        );
        let (start, end, page) = page(rows.len(), options.limit, options.cursor.as_deref(), &key)?;
        let mut report = self.envelope("diagrams");
        report["diagram_schema"] = json!(DIAGRAM_SCHEMA);
        report["items"] = json!(&rows[start..end]);
        report["page"] = page;
        report["analysis"] = json!({
            "documents": document_count, "headings": heading_count, "diagrams": diagram_count,
            "nodes": node_count, "edges": edge_count, "gaps": gap_count,
            "node_limit_per_diagram": MERMAID_NODE_LIMIT, "edge_limit_per_diagram": MERMAID_EDGE_LIMIT,
            "nodes_omitted": nodes_omitted, "edges_omitted": edges_omitted,
            "certainty": "Markdown hierarchy and Mermaid identifiers preserve captured syntax positions; renderer semantics remain unchecked.",
        });
        Ok(report)
    }
}
