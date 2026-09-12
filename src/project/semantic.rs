use super::{bounded_text, hash, Project};
use crate::model::{Symbol, SymbolKind};
use crate::parse::Parsers;
use crate::transpile::ir::{Function, Item, Module};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "fr-semantic-model-1";
const MAX_SEMANTIC_SOURCE_BYTES: usize = 262_144;

#[derive(Args)]
pub struct Options {
    #[arg(
        default_value = ".",
        help = "Indexed file path or revision-bound declaration handle."
    )]
    pub(super) target: String,
    #[arg(long, help = "Required when TARGET is a short project node ID.")]
    pub(super) revision: Option<String>,
    #[arg(
        long,
        help = "Select one unique declaration name inside a path target."
    )]
    pub(super) declaration: Option<String>,
    #[arg(
        long,
        help = "Return complete supported bodies and their detected patterns."
    )]
    pub(super) body: bool,
    #[arg(
        long,
        requires = "body",
        help = "List exact typed pointers for the selected authorable body."
    )]
    pub(super) pointers: bool,
    #[arg(
        long,
        default_value_t = 256,
        help = "Maximum complete semantic nodes returned, from 1 through 4096."
    )]
    pub(super) nodes: usize,
    #[arg(
        long,
        help = "Include source carried by unsupported or fallback IR nodes."
    )]
    pub(super) unsupported_source: bool,
    #[arg(
        long,
        help = "Omit the generic project envelope after binding semantic identity."
    )]
    pub(super) minimal: bool,
}

pub fn semantic_section_fits(required: usize, budget: usize) -> bool {
    required <= budget
}

pub(super) fn selected_function(module: &Module, symbol: &Symbol) -> Option<Function> {
    let record_matches = |name: &str| {
        symbol.qualifier.as_deref().is_none_or(|qualifier| {
            qualifier == name
                || qualifier.rsplit("::").next() == Some(name)
                || qualifier.rsplit('.').next() == Some(name)
        })
    };
    match symbol.kind {
        SymbolKind::Function => module.items.iter().find_map(|item| match item {
            Item::Function(function) if function.name == symbol.name => Some(function.clone()),
            _ => None,
        }),
        SymbolKind::Method => module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Record(record) if record_matches(&record.name) => record
                    .methods
                    .iter()
                    .find(|method| method.name == symbol.name)
                    .cloned(),
                _ => None,
            })
            .or_else(|| {
                let mut matches = module.items.iter().filter_map(|item| match item {
                    Item::Function(function) if function.name == symbol.name => Some(function),
                    _ => None,
                });
                let only = matches.next()?.clone();
                matches.next().is_none().then_some(only)
            }),
        _ => None,
    }
}

fn selected_item(module: &Module, symbol: &Symbol) -> Option<Item> {
    match symbol.kind {
        SymbolKind::Function | SymbolKind::Method => {
            selected_function(module, symbol).map(Item::Function)
        }
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Trait | SymbolKind::Interface => {
            module.items.iter().find_map(|item| match item {
                Item::Record(record) if record.name == symbol.name => Some(item.clone()),
                _ => None,
            })
        }
        SymbolKind::Enum => module.items.iter().find_map(|item| match item {
            Item::Sum(sum) if sum.name == symbol.name => Some(item.clone()),
            _ => None,
        }),
        SymbolKind::TypeAlias => module.items.iter().find_map(|item| match item {
            Item::Newtype(newtype) if newtype.name == symbol.name => Some(item.clone()),
            _ => None,
        }),
        SymbolKind::Constant => module.items.iter().find_map(|item| match item {
            Item::Constant(constant) if constant.name == symbol.name => Some(item.clone()),
            _ => None,
        }),
        _ => None,
    }
}

fn omit_bodies(module: &mut Module) -> (usize, usize) {
    let mut bodies = 0usize;
    let mut statements = 0usize;
    module.doc.clear();
    module.sweep_notes.clear();
    module.items.retain_mut(|item| match item {
        Item::Function(function) => {
            bodies += usize::from(!function.body.is_empty());
            statements += function.body.len();
            function.doc.clear();
            function.body.clear();
            true
        }
        Item::Record(record) => {
            record.doc.clear();
            for field in &mut record.fields {
                field.doc.clear();
            }
            for method in &mut record.methods {
                bodies += usize::from(!method.body.is_empty());
                statements += method.body.len();
                method.doc.clear();
                method.body.clear();
            }
            true
        }
        Item::Test { doc, body, .. } => {
            bodies += usize::from(!body.is_empty());
            statements += body.len();
            doc.clear();
            body.clear();
            true
        }
        Item::Statement(_) => {
            statements += 1;
            false
        }
        Item::Constant(constant) => {
            constant.doc.clear();
            true
        }
        Item::Newtype(newtype) => {
            newtype.doc.clear();
            true
        }
        Item::Sum(sum) => {
            sum.doc.clear();
            for variant in &mut sum.variants {
                variant.doc.clear();
                for field in &mut variant.fields {
                    field.doc.clear();
                }
            }
            true
        }
        Item::Import { .. } | Item::Unsupported(_) => true,
    });
    (bodies, statements)
}

fn redact_sources(value: &mut Value, redacted: &mut usize) {
    match value {
        Value::Array(values) => {
            for value in values {
                redact_sources(value, redacted);
            }
        }
        Value::Object(object) => {
            if let Some(Value::String(source)) = object.remove("source") {
                object.insert("source_bytes".into(), json!(source.len()));
                object.insert(
                    "source_sha256".into(),
                    json!(format!("{:x}", Sha256::digest(source.as_bytes()))),
                );
                *redacted += 1;
            }
            if object.get("kind") == Some(&Value::String("import".into())) {
                if let Some(Value::Object(import)) = object.get_mut("value") {
                    if let Some(Value::String(text)) = import.remove("text") {
                        import.insert("source_bytes".into(), json!(text.len()));
                        import.insert(
                            "source_sha256".into(),
                            json!(format!("{:x}", Sha256::digest(text.as_bytes()))),
                        );
                        *redacted += 1;
                    }
                }
            }
            for value in object.values_mut() {
                redact_sources(value, redacted);
            }
        }
        _ => {}
    }
}

pub(super) fn semantic_nodes(value: &Value) -> usize {
    match value {
        Value::Array(values) => values.iter().map(semantic_nodes).sum(),
        Value::Object(object) => {
            usize::from(object.get("kind").is_some())
                + object.values().map(semantic_nodes).sum::<usize>()
        }
        _ => 0,
    }
}

fn patterns(value: &Value, pointer: &str, basis: &str, out: &mut Vec<Value>) {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                patterns(value, &format!("{pointer}/{index}"), basis, out);
            }
        }
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str) {
                let pattern =
                    match kind {
                        "comprehension" => {
                            let filtered =
                                object.get("value").and_then(Value::as_object).is_some_and(
                                    |value| !value.get("condition").is_none_or(Value::is_null),
                                );
                            Some(if filtered { "filter-map" } else { "map" })
                        }
                        "if-present" => Some("optional-branch"),
                        "while-present" => Some("optional-loop"),
                        "match-variants" => Some("variant-match"),
                        "propagate" => Some("failure-propagation"),
                        "defer" => Some("deferred-cleanup"),
                        "err-defer" => Some("failure-cleanup"),
                        "try" => Some("exception-region"),
                        "await" => Some("suspension"),
                        "lambda" => Some("higher-order-function"),
                        _ => None,
                    };
                if let Some(pattern) = pattern {
                    out.push(json!({
                        "pattern": pattern,
                        "node_kind": kind,
                        "address": format!("{basis}#{pointer}"),
                        "basis": "syntax-derived-ir-shape",
                        "behavior_proved": false
                    }));
                }
            }
            for (key, value) in object {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                patterns(value, &format!("{pointer}/{escaped}"), basis, out);
            }
        }
        _ => {}
    }
}

impl Project<'_> {
    pub(super) fn semantic(&self, options: &Options) -> Result<Value> {
        ensure!(
            (1..=4096).contains(&options.nodes),
            "semantic node budget must be between 1 and 4096."
        );
        ensure!(
            options.declaration.is_none()
                || !options.target.starts_with("frp1:") && options.revision.is_none(),
            "--declaration requires a path target without --revision."
        );
        let mut target = if options.target.starts_with("frp1:") || options.revision.is_some() {
            self.resolve_handle(
                &self.explicit_handle(&options.target, options.revision.as_deref())?,
            )?
        } else {
            self.target(&options.target)?
        };
        if let Some(name) = &options.declaration {
            ensure!(
                self.nodes[target].kind == "file",
                "--declaration requires a file path target."
            );
            let path = &self.nodes[target].path;
            let mut matches = self.nodes.iter().enumerate().filter_map(|(id, node)| {
                let symbol = node.symbol.and_then(|symbol| self.index.symbol(symbol))?;
                (node.path == *path
                    && symbol.name == *name
                    && matches!(
                        symbol.kind,
                        SymbolKind::Function
                            | SymbolKind::Method
                            | SymbolKind::Class
                            | SymbolKind::Struct
                            | SymbolKind::Trait
                            | SymbolKind::Interface
                            | SymbolKind::Enum
                            | SymbolKind::TypeAlias
                            | SymbolKind::Constant
                    ))
                .then_some(id)
            });
            target = matches
                .next()
                .with_context(|| format!("no semantic declaration named '{name}' in the file."))?;
            ensure!(
                matches.next().is_none(),
                "semantic declaration name is ambiguous; select a revision-bound handle."
            );
        }
        let node = &self.nodes[target];
        ensure!(
            node.kind != "directory",
            "semantic queries require a file or declaration."
        );
        let path = self.root.join(&node.path);
        let file = self
            .index
            .file(&path)
            .context("semantic target is not indexed.")?;
        ensure!(
            crate::transpile::can_be_read(file.language),
            "{} has no semantic IR reader.",
            file.language
        );
        let source = self
            .sources
            .get(&path)
            .context("semantic source is unavailable.")?;
        ensure!(
            source.len() <= MAX_SEMANTIC_SOURCE_BYTES,
            "semantic source exceeds the 262144-byte analysis limit; select a smaller file or use structural project queries."
        );
        let parsed = Parsers::new().parse(file.language, source)?;
        ensure!(
            !parsed.has_errors(),
            "semantic source contains parser errors."
        );
        let mut module = crate::transpile::read_module(file.language, source, parsed.root())?;
        let mut body_identity = None;
        let mut pointer_body = None;
        let selected = if let Some(symbol) = node.symbol.and_then(|id| self.index.symbol(id)) {
            let item = selected_item(&module, symbol).with_context(|| {
                format!(
                    "the {} '{}' has no exact semantic IR item; select its file or request bounded source.",
                    symbol.kind.as_str(), symbol.name
                )
            })?;
            ensure!(
                !options.pointers || matches!(item, Item::Function(_)),
                "semantic body pointers require one function or method declaration."
            );
            if let Item::Function(function) = &item {
                let body = super::semantic_change::SemanticBody {
                    schema: super::semantic_ir::BODY_SCHEMA.into(),
                    body: function.body.clone(),
                };
                let value = serde_json::to_value(&body)?;
                let nodes = semantic_nodes(&value);
                body_identity = Some(if !super::semantic_ir::source_free(&value) {
                    json!({"status":"unavailable-source-bearing","source_free":false})
                } else if body.body.len() > super::semantic_change::MAX_STATEMENTS
                    || nodes > super::semantic_change::MAX_NODES
                {
                    json!({"status":"unavailable-size","source_free":true,
                        "statements":body.body.len(),"semantic_nodes":nodes})
                } else {
                    let basis = super::semantic_change::body_basis(&body)?;
                    if options.pointers {
                        pointer_body = Some(body.clone());
                    }
                    json!({"status":"available","source_free":true,
                        "schema":super::semantic_ir::BODY_SCHEMA,
                        "basis":basis,
                        "statements":body.body.len(),"semantic_nodes":nodes})
                });
            }
            module.items = vec![item];
            "declaration"
        } else {
            ensure!(
                !options.pointers,
                "semantic body pointers require one function or method declaration."
            );
            "file"
        };
        let (omitted_bodies, omitted_statements) = if options.body {
            (0, 0)
        } else {
            omit_bodies(&mut module)
        };
        let mut model = serde_json::to_value(&module)?;
        let mut redacted = 0usize;
        if !options.unsupported_source {
            redact_sources(&mut model, &mut redacted);
        }
        let required = semantic_nodes(&model).max(1);
        ensure!(
            !options.pointers || semantic_section_fits(required, options.nodes),
            "semantic body pointers require a complete body within the node budget."
        );
        let payload_hash = hash((&self.revision, self.handle(target), &model))?;
        let basis = format!("frsm1:{payload_hash}");
        let mut report = self.envelope("semantic");
        report["semantic_schema"] = json!(SCHEMA);
        report["semantic_basis"] = json!(basis);
        report["selection"] = json!({
            "kind": selected,
            "handle": self.handle(target),
            "path": bounded_text(&node.path.to_string_lossy(), 512),
            "language": file.language,
            "body_requested": options.body
        });
        if let Some(identity) = body_identity {
            report["body_identity"] = identity;
        }
        if options.pointers {
            report["body_pointers"] = if let Some(body) = pointer_body {
                json!({
                    "schema":"fr-semantic-body-pointers-1",
                    "basis":report["body_identity"]["basis"],
                    "fields":["path","category","kind"],
                    "rows":super::semantic_change::body_pointers(&body)?
                })
            } else {
                json!({"schema":"fr-semantic-body-pointers-1","status":"unavailable-body-identity"})
            };
        }
        report["addressing"] = json!({
            "format": "SEMANTIC_BASIS#RFC6901_JSON_POINTER",
            "root": format!("{}#/model", report["semantic_basis"].as_str().unwrap())
        });
        report["node_budget"] = json!({
            "limit": options.nodes,
            "required": required,
            "unit": "tagged semantic nodes",
            "complete_subtrees_only": true
        });
        report["analysis_budget"] = json!({
            "source_bytes": source.len(),
            "maximum_source_bytes": MAX_SEMANTIC_SOURCE_BYTES
        });
        report["omitted"] = json!({
            "bodies": omitted_bodies,
            "top_level_statements": omitted_statements,
            "source_fields": redacted
        });
        report["source_policy"] = json!(if options.unsupported_source {
            "explicit-unsupported-source"
        } else {
            "source-free"
        });
        report["scope"] = json!(
            "Syntax-derived cross-language IR; types can be inferred from local evidence. Pattern rows do not prove behavior."
        );
        if semantic_section_fits(required, options.nodes) {
            let mut found = Vec::new();
            patterns(
                &model,
                "/model",
                report["semantic_basis"].as_str().unwrap(),
                &mut found,
            );
            report["status"] = json!("returned");
            report["model"] = model;
            report["patterns"] = json!(found);
        } else {
            report["status"] = json!("omitted-node-budget");
            report["model"] = Value::Null;
            report["patterns"] = json!([]);
        }
        Ok(report)
    }
}

pub(crate) fn minimize(command: &super::Command, report: &mut Value) {
    let super::Command::Semantic(options) = command else {
        return;
    };
    if !options.minimal {
        return;
    }
    let omitted = [
        "schema",
        "revision",
        "handle_prefix",
        "root",
        "coverage",
        "context_basis",
        "addressing",
        "analysis_budget",
        "scope",
    ];
    for field in omitted {
        report.as_object_mut().unwrap().remove(field);
    }
    report["report_omitted"] = json!(omitted);
}
