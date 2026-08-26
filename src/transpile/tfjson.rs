//! Terraform's two syntaxes for one configuration.
//!
//! HCL has an official JSON syntax, and Terraform reads `.tf` and `.tf.json` as
//! two spellings of the same thing. A generator writes the JSON one because
//! writing JSON is easy; a person reads the HCL one because reading HCL is
//! easy. A workspace holds both, and moving a file from one to the other is a
//! real conversion and not a rename: the block header
//!
//! ```hcl
//! resource "aws_s3_bucket" "b" {
//!   acl = "private"
//! }
//! ```
//!
//! becomes nesting, one level per label:
//!
//! ```json
//! { "resource": { "aws_s3_bucket": { "b": { "acl": "private" } } } }
//! ```
//!
//! What crosses exactly: blocks, their labels, attributes, and the literals
//! `string`, `number`, `bool` and `null`. What does not: an expression. `type =
//! bool` and `count = var.n + 1` are HCL, and the JSON syntax writes each as a
//! string for Terraform to re-parse. Reading that string back is a decision
//! about whether the author meant text or an expression. So an expression
//! crosses as the string Terraform expects and comes back as an expression, and
//! the round trip says which of the two it read.

use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use serde_json::{Map, Value};
use tree_sitter::Node;

/// The JSON form of an HCL configuration.
pub fn to_json(source: &str) -> Result<String> {
    let parsed = Parsers::new().parse(Language::Hcl, source)?;
    if parsed.has_errors() {
        bail!("the hcl does not parse, so there is nothing to convert");
    }
    let root = parsed.tree.root_node();
    let Some(body) = child_of_kind(root, "body") else {
        return Ok("{}\n".to_string());
    };
    let value = Value::Object(body_to_json(body, source)?);
    Ok(format!("{}\n", serde_json::to_string_pretty(&value)?))
}

/// The HCL form of a Terraform JSON configuration.
pub fn to_hcl(source: &str) -> Result<String> {
    let value: Value = serde_json::from_str(source)?;
    let Value::Object(top) = value else {
        bail!("a terraform configuration is an object at the top, and this is not");
    };
    let mut out = String::new();
    for (name, held) in &top {
        if !out.is_empty() {
            out.push('\n');
        }
        write_block(&mut out, name, held, 0);
    }
    Ok(out)
}

/// The first child of this kind, where there is one.
fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

/// Every body, as the object its attributes and blocks make.
///
/// Two blocks with the same header are a list, which is what Terraform means by
/// writing the header twice. One block is the object on its own, which is what
/// Terraform's own JSON syntax accepts either way.
fn body_to_json(body: Node<'_>, source: &str) -> Result<Map<String, Value>> {
    let mut out: Map<String, Value> = Map::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "attribute" => {
                let mut inner = child.walk();
                let parts: Vec<Node> = child.children(&mut inner).collect();
                let Some(name) = parts.first().map(|n| source[n.byte_range()].to_string()) else {
                    continue;
                };
                let Some(expression) = parts.iter().find(|n| n.kind() == "expression") else {
                    continue;
                };
                out.insert(name, expression_to_json(*expression, source));
            }
            "block" => {
                let (labels, inner) = block_parts(child, source);
                let Some(inner) = inner else { continue };
                let held = Value::Object(body_to_json(inner, source)?);
                // Each label is a level of nesting, innermost last.
                let mut wrapped = held;
                for label in labels.iter().skip(1).rev() {
                    let mut level = Map::new();
                    level.insert(label.clone(), wrapped);
                    wrapped = Value::Object(level);
                }
                let Some(head) = labels.first() else { continue };
                // The labels are levels of nesting, and two blocks sharing them
                // share those levels. Two blocks sharing *every* label are two
                // of the same thing, which is where a list begins.
                merge(&mut out, head, wrapped, labels.len().saturating_sub(1));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A block's header words and the body under them.
fn block_parts<'t>(block: Node<'t>, source: &str) -> (Vec<String>, Option<Node<'t>>) {
    let mut labels = Vec::new();
    let mut body = None;
    let mut cursor = block.walk();
    for child in block.children(&mut cursor) {
        match child.kind() {
            "identifier" => labels.push(source[child.byte_range()].to_string()),
            "string_lit" => labels.push(unquoted(&source[child.byte_range()])),
            "body" => body = Some(child),
            _ => {}
        }
    }
    (labels, body)
}

/// Put a value under a key, merging down the levels its labels name.
///
/// `resource "aws_s3_bucket" "one"` and `resource "aws_s3_bucket" "two"` share
/// two levels and part at the third, so the type holds both names. Two blocks
/// sharing every label are two of the same thing, and Terraform's JSON syntax
/// says that with an array. Overwriting would lose the first silently.
fn merge(into: &mut Map<String, Value>, key: &str, value: Value, levels: usize) {
    let Some(already) = into.remove(key) else {
        into.insert(key.to_string(), value);
        return;
    };
    let merged = match (already, value, levels) {
        // Another label below: the two go on sharing this level.
        (Value::Object(mut first), Value::Object(second), left) if left > 0 => {
            for (name, held) in second {
                merge(&mut first, &name, held, left - 1);
            }
            Value::Object(first)
        }
        (Value::Array(mut held), value, _) => {
            held.push(value);
            Value::Array(held)
        }
        (first, second, _) => Value::Array(vec![first, second]),
    };
    into.insert(key.to_string(), merged);
}

/// A literal as the JSON value it is, and anything else as the string
/// Terraform's JSON syntax expects.
fn expression_to_json(expression: Node<'_>, source: &str) -> Value {
    let text = source[expression.byte_range()].trim();
    let literal = child_of_kind(expression, "literal_value").and_then(|l| {
        let mut cursor = l.walk();
        let first = l.children(&mut cursor).next();
        first
    });
    match literal.map(|l| l.kind()) {
        Some("string_lit") => Value::String(unquoted(text)),
        Some("numeric_lit") => match text.parse::<i64>() {
            Ok(n) => Value::Number(n.into()),
            Err(_) => match text.parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
                Some(n) => Value::Number(n),
                None => Value::String(text.to_string()),
            },
        },
        Some("bool_lit") => Value::Bool(text == "true"),
        Some("null_lit") => Value::Null,
        // An expression: `var.n`, `bool`, `a + 1`. Terraform's JSON syntax
        // holds one as a string and re-parses it, which is what this writes.
        _ => Value::String(text.to_string()),
    }
}

/// The text between a string literal's quotes.
fn unquoted(text: &str) -> String {
    let text = text.trim();
    match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(inside) => inside.to_string(),
        None => text.to_string(),
    }
}

/// One block, and the levels of nesting that are its labels.
fn write_block(out: &mut String, name: &str, value: &Value, depth: usize) {
    let pad = "  ".repeat(depth);
    match value {
        // Nesting under a name is a block whose labels are the keys, down to
        // the object that holds the attributes.
        Value::Object(fields) => {
            let labels = block_labels(fields);
            match labels {
                Some((label, inner)) => {
                    write_block_with(out, name, &[label], inner, depth);
                }
                None => {
                    out.push_str(&format!("{pad}{name} {{\n"));
                    write_body(out, fields, depth + 1);
                    out.push_str(&format!("{pad}}}\n"));
                }
            }
        }
        // An array under a name is that block written more than once.
        Value::Array(items) => {
            for item in items {
                write_block(out, name, item, depth);
            }
        }
        other => out.push_str(&format!("{pad}{name} = {}\n", literal(other))),
    }
}

/// The one label this level names, where the level is only labels.
///
/// `{"aws_s3_bucket": {"b": {…}}}` is two labels and then a body. A level with
/// several keys is a body already: those keys are attributes or nested blocks.
fn block_labels(fields: &Map<String, Value>) -> Option<(String, &Map<String, Value>)> {
    let mut entries = fields.iter();
    let (name, value) = entries.next()?;
    if entries.next().is_some() {
        return None;
    }
    match value {
        Value::Object(inner) => Some((name.clone(), inner)),
        _ => None,
    }
}

/// A block written with the labels collected so far.
fn write_block_with(
    out: &mut String,
    name: &str,
    labels: &[String],
    inner: &Map<String, Value>,
    depth: usize,
) {
    // Keep peeling while a level names one key holding an object, which is a
    // label and not a body.
    if let Some((next, deeper)) = block_labels(inner) {
        let mut carried = labels.to_vec();
        carried.push(next);
        write_block_with(out, name, &carried, deeper, depth);
        return;
    }
    let pad = "  ".repeat(depth);
    let quoted: Vec<String> = labels.iter().map(|l| format!("\"{l}\"")).collect();
    out.push_str(&format!("{pad}{name} {} {{\n", quoted.join(" ")));
    write_body(out, inner, depth + 1);
    out.push_str(&format!("{pad}}}\n"));
}

/// The attributes and nested blocks of one body.
fn write_body(out: &mut String, fields: &Map<String, Value>, depth: usize) {
    let pad = "  ".repeat(depth);
    for (name, value) in fields {
        match value {
            Value::Object(_) | Value::Array(_) => write_block(out, name, value, depth),
            other => out.push_str(&format!("{pad}{name} = {}\n", literal(other))),
        }
    }
}

/// A JSON scalar as HCL writes one.
///
/// A string that reads as an expression is written as one. Terraform's JSON
/// syntax holds `"var.n"` for an author who wrote `var.n`, and putting the
/// quotes back would change what the configuration means.
fn literal(value: &Value) -> String {
    match value {
        Value::String(text) => match reads_as_an_expression(text) {
            true => text.clone(),
            false => format!("\"{text}\""),
        },
        Value::Bool(yes) => yes.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// The type keywords, which are the one bare word Terraform reads as an
/// expression rather than as text.
const TYPE_WORDS: &[&str] = &[
    "string", "number", "bool", "any", "list", "map", "set", "object", "tuple",
];

/// Would Terraform re-parse this string as an expression?
///
/// A dotted path, an interpolation, or a type keyword. A bare word on its own
/// is *not* one: `acl = "private"` and `type = bool` look the same in JSON, and
/// reading the first as an expression would leave `acl = private`, a reference
/// to something the configuration never declares. So a lone word stays a
/// string, and the words that are types are named.
fn reads_as_an_expression(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.contains("${") {
        return true;
    }
    if text.contains(' ') {
        return false;
    }
    if TYPE_WORDS.contains(&text) {
        return true;
    }
    // `var.name`, `local.a.b`: identifiers separated by dots, at least two of
    // them, and nothing else. `example.com` is a hostname, so the head has to
    // be one of the names Terraform itself puts a reference under.
    let mut parts = text.split('.');
    let Some(head) = parts.next() else {
        return false;
    };
    let heads = ["var", "local", "module", "data", "each", "count", "self", "path"];
    if !heads.contains(&head) {
        return false;
    }
    let rest: Vec<&str> = parts.collect();
    !rest.is_empty()
        && rest
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_'))
}
