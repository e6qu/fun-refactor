use super::{
    bounded_text, fast_routes::children, fast_routes::text, hash, Project, RelationshipOptions,
};
use crate::lang::Language;
use crate::model::{Symbol, SymbolKind};
use crate::parse::{Parsed, Parsers};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

fn schema_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::TypeAlias | SymbolKind::Struct
    )
}

fn declaration<'a>(parsed: &'a Parsed, symbol: &Symbol) -> Option<Node<'a>> {
    let mut node = parsed.node_at(symbol.name_span.start)?;
    loop {
        if matches!(
            node.kind(),
            "class_definition"
                | "interface_declaration"
                | "type_alias_declaration"
                | "struct_item"
                | "type_item"
        ) && node.child_by_field_name("name").is_some_and(|name| {
            name.start_byte() == symbol.name_span.start && name.end_byte() == symbol.name_span.end
        }) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

fn unwrapped(node: Node<'_>) -> Node<'_> {
    if matches!(node.kind(), "type" | "type_annotation") {
        children(node).first().copied().unwrap_or(node)
    } else {
        node
    }
}

fn annotated(node: Node<'_>, source: &str) -> bool {
    node.kind() == "generic_type"
        && children(node)
            .first()
            .is_some_and(|name| text(*name, source).rsplit('.').next() == Some("Annotated"))
}

fn field_type<'a>(node: Node<'a>, source: &str, python: bool) -> Option<Node<'a>> {
    let node = unwrapped(node);
    if python && annotated(node, source) {
        children(node)
            .into_iter()
            .find(|n| n.kind() == "type_parameter")
            .and_then(|args| children(args).first().copied())
            .map(unwrapped)
    } else {
        Some(node)
    }
}

fn type_names(
    node: Node<'_>,
    source: &str,
    language: Language,
    depth: usize,
    names: &mut BTreeSet<String>,
) -> bool {
    if depth > 16 || (language == Language::Python && annotated(node, source)) {
        return false;
    }
    if language == Language::Rust
        && node.kind() == "array_type"
        && node.child_by_field_name("length").is_some()
    {
        return false;
    }
    let mut cursor = node.walk();
    if node.children(&mut cursor).any(|n| n.is_extra()) {
        return false;
    }
    match node.kind() {
        "identifier" | "type_identifier" | "attribute" | "nested_type_identifier" => {
            let name = text(node, source);
            if !name.split('.').all(super::contracts::simple_name) {
                return false;
            }
            names.insert(name.to_owned());
            true
        }
        "scoped_type_identifier" if language == Language::Rust => {
            let name = text(node, source);
            if !name
                .strip_prefix("::")
                .unwrap_or(name)
                .split("::")
                .all(super::contracts::simple_name)
            {
                return false;
            }
            names.insert(name.to_owned());
            true
        }
        "none" | "predefined_type" | "null" | "primitive_type" | "unit_type" | "lifetime"
        | "mutable_specifier" => true,
        "literal_type" => children(node).as_slice().iter().all(|n| n.kind() == "null"),
        "type" | "type_annotation" | "generic_type" | "type_parameter" | "type_arguments"
        | "union_type" | "intersection_type" | "array_type" | "tuple_type"
        | "parenthesized_type" | "reference_type" | "pointer_type" | "slice_type" => {
            let parts = children(node);
            !parts.is_empty()
                && parts
                    .into_iter()
                    .all(|n| type_names(n, source, language, depth + 1, names))
        }
        "binary_operator"
            if node
                .child_by_field_name("operator")
                .is_some_and(|n| text(n, source) == "|") =>
        {
            children(node)
                .into_iter()
                .all(|n| type_names(n, source, language, depth + 1, names))
        }
        _ => false,
    }
}

pub(super) fn declared_type_names(
    node: Node<'_>,
    source: &str,
    language: Language,
) -> Option<BTreeSet<String>> {
    if !matches!(
        language,
        Language::Python | Language::TypeScript | Language::Tsx | Language::Rust
    ) {
        return None;
    }
    let node = field_type(node, source, language == Language::Python)?;
    let mut names = BTreeSet::new();
    type_names(node, source, language, 0, &mut names).then_some(names)
}

fn gap(schema: &Value, line: usize, reason: &'static str) -> Value {
    json!({"kind": "schema-gap", "schema": schema["handle"], "line": line,
        "basis": "declared-schema-reader", "reason": reason})
}

impl Project<'_> {
    pub(super) fn type_candidates(&self, name: &str, file: &std::path::Path) -> Vec<&Symbol> {
        self.index
            .find_symbols(name, Some(file))
            .into_iter()
            .filter(|s| schema_kind(s.kind))
            .collect()
    }

    fn schema_field(
        &self,
        symbol: &Symbol,
        schema: &Value,
        node: Node<'_>,
        source: &str,
        python: bool,
    ) -> Result<Vec<Value>> {
        let line = node.start_position().row + 1;
        let name = node.child_by_field_name(if python { "left" } else { "name" });
        let Some(name) = name.filter(|n| {
            matches!(
                n.kind(),
                "identifier" | "property_identifier" | "field_identifier"
            )
        }) else {
            return Ok(vec![gap(
                schema,
                line,
                "Computed, quoted and destructured field names are outside this reader.",
            )]);
        };
        let id = format!(
            "frpsf1:{}",
            &hash((&self.revision, &schema["handle"], node.start_byte()))?[..32]
        );
        let mut names = BTreeSet::new();
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| field_type(n, source, python))
            .filter(|n| type_names(*n, source, symbol.language, 0, &mut names));
        let typescript = matches!(symbol.language, Language::TypeScript | Language::Tsx);
        let mut cursor = node.walk();
        let optional = typescript.then(|| node.children(&mut cursor).any(|n| n.kind() == "?"));
        let mut cursor = node.walk();
        let readonly =
            typescript.then(|| node.children(&mut cursor).any(|n| n.kind() == "readonly"));
        let mut rows = vec![
            json!({"kind": "schema-field", "id": id, "schema": schema["handle"],
            "name": bounded_text(text(name, source), 160), "line": line,
            "declared_type": ty.map(|n| bounded_text(text(n, source), 512)),
            "optional_marker": optional, "readonly_marker": readonly, "required": null,
            "basis": "direct-field-declaration", "status": "candidate", "confidence": null}),
        ];
        if ty.is_none() {
            rows.push(gap(schema, line, "The field has no supported type spelling; computed, literal and complex types remain unknown."));
            return Ok(rows);
        }
        for name in names {
            let candidates = self.type_candidates(&name, &symbol.file);
            let reference = format!("frpst1:{}", &hash((&id, &name))?[..32]);
            rows.push(json!({"kind": "schema-type-reference", "id": reference, "schema": schema["handle"],
                "field": id, "name": bounded_text(&name, 160), "line": line,
                "candidate_count": candidates.len(), "status": match candidates.len() { 0 => "unresolved", 1 => "candidate", _ => "ambiguous" },
                "basis": "same-file-type-name", "confidence": null}));
            for candidate in candidates {
                rows.push(
                    json!({"kind": "schema-type-candidate", "reference": reference,
                    "target": self.endpoint(candidate.id)?, "basis": "same-file-type-name",
                    "status": "candidate", "confidence": "name-only"}),
                );
            }
        }
        Ok(rows)
    }

    fn schema_rows(&self, symbol: &Symbol, parsed: &Parsed, source: &str) -> Result<Vec<Value>> {
        let schema = self.endpoint(symbol.id)?;
        let line = schema["line"].as_u64().unwrap_or(1) as usize;
        let python = symbol.language == Language::Python;
        let Some(declaration) = declaration(parsed, symbol) else {
            return Ok(vec![gap(&schema, line, "This declaration form is outside the Python class, TypeScript object and Rust struct reader.")]);
        };
        let body =
            declaration.child_by_field_name(if declaration.kind() == "type_alias_declaration" {
                "value"
            } else {
                "body"
            });
        let members = match body {
            Some(body) if matches!(body.kind(), "block" | "interface_body" | "object_type" | "field_declaration_list") => children(body),
            None if declaration.kind() == "struct_item" => Vec::new(),
            _ => return Ok(vec![gap(&schema, line, "Only direct object type aliases and named or unit Rust structs have fields in this reader.")]),
        };
        let mut rows = vec![gap(&schema, line, "Wire names, requiredness, validation, serialization and runtime schema identity remain unchecked.")];
        if symbol.language == Language::Rust {
            rows.push(gap(&schema, line, "Rust attributes, derives, cfg conditions, visibility and generic bounds remain unchecked; attribute values stay outside the output."));
        }
        if declaration.child_by_field_name("superclasses").is_some()
            || declaration.child_by_field_name("type_parameters").is_some()
            || children(declaration)
                .iter()
                .any(|n| n.kind() == "extends_type_clause")
        {
            rows.push(gap(&schema, line, "Inheritance and generic substitutions are not expanded; same-file names do not establish lexical type resolution."));
        }
        if declaration
            .parent()
            .is_some_and(|n| n.kind() == "decorated_definition")
        {
            rows.push(gap(&schema, line, "Class decorators may change fields and behavior; the reader omits decorator metadata."));
        }
        let mut field_names = BTreeSet::new();
        for member in members {
            let field = if python && member.kind() == "expression_statement" {
                children(member)
                    .first()
                    .copied()
                    .filter(|n| n.kind() == "assignment" && n.child_by_field_name("type").is_some())
            } else {
                matches!(member.kind(), "property_signature" | "field_declaration")
                    .then_some(member)
            };
            if let Some(field) = field {
                if let Some(name) = field.child_by_field_name(if python { "left" } else { "name" })
                {
                    if !field_names.insert(text(name, source)) {
                        rows.push(gap(&schema, field.start_position().row + 1, "Duplicate field declarations remain separate; overriding and merging are unchecked."));
                    }
                }
                rows.extend(self.schema_field(symbol, &schema, field, source, python)?);
            } else if member.kind() != "pass_statement"
                && !(python
                    && member.kind() == "expression_statement"
                    && children(member).iter().all(|n| n.kind() == "string"))
            {
                rows.push(gap(&schema, member.start_position().row + 1, "A member is outside direct annotated fields; the reader omits methods, validators, configuration and computed members."));
            }
        }
        let count = |kind| rows.iter().filter(|r| r["kind"] == kind).count();
        rows.push(json!({"kind": "schema", "declaration": schema, "status": "candidate", "completeness": "partial",
            "field_count": count("schema-field"), "type_references": count("schema-type-reference"), "gap_count": count("schema-gap"),
            "basis": match symbol.language { Language::Python => "python-class-annotations", Language::Rust => "rust-struct-declaration", _ => "typescript-object-declaration" }, "confidence": null}));
        Ok(rows)
    }

    pub(super) fn schemas(&self, options: &RelationshipOptions) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        let mut rows = Vec::new();
        let mut unsupported = BTreeMap::new();
        let mut analyzed = 0usize;
        let parsers = Parsers::new();
        for (file, info) in self.index.files() {
            if !self.scope_file(selected, file) {
                continue;
            }
            if !matches!(
                info.language,
                Language::Python | Language::TypeScript | Language::Tsx | Language::Rust
            ) {
                *unsupported.entry(info.language.name()).or_insert(0usize) += 1;
                continue;
            }
            let source = &self.sources[file];
            let parsed = parsers.parse(info.language, source)?;
            if parsed.has_errors() {
                rows.push(json!({"kind": "analysis-gap", "path": bounded_text(&file.strip_prefix(&self.root)?.to_string_lossy(), 512),
                    "basis": "declared-schema-reader", "reason": "Source has syntax errors; the reader skipped this file."}));
                continue;
            }
            analyzed += 1;
            for symbol in info
                .symbols
                .iter()
                .filter_map(|id| self.index.symbol(*id))
                .filter(|s| schema_kind(s.kind) && self.scope_symbol(selected, s.id))
            {
                rows.extend(self.schema_rows(symbol, &parsed, source)?);
            }
        }
        for (language, files) in &unsupported {
            rows.push(json!({"kind": "coverage-gap", "language": language, "files": files,
                "reason": "Declared schema inspection supports Python classes, TypeScript interfaces/object aliases and named or unit Rust structs."}));
        }
        let analysis = json!({"analyzed_files": analyzed, "unsupported_language_files": unsupported,
            "declarations": rows.iter().filter(|r| r["kind"] == "schema").count(),
            "fields": rows.iter().filter(|r| r["kind"] == "schema-field").count(),
            "type_references": rows.iter().filter(|r| r["kind"] == "schema-type-reference").count(),
            "limitations": ["Direct declarations are partial candidates, not inferred wire schemas.",
                "Same-file type-name candidates preserve duplicates; lexical scope, imports, builtins and type parameters are unresolved.",
                "No recursive expansion, inherited fields, validators, default values or method bodies.",
                "Output is paged; indexing and selected-file analysis work are not bounded by the page limit.",
                "Shared Lean paging laws apply; schema extraction and type resolution are outside those proofs."]});
        self.relationship_page("schemas", selected, options, None, rows, analysis)
    }
}
