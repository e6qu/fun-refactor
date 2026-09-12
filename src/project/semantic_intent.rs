use super::semantic_change::{self, NodeCategory, SemanticBody};
use crate::transpile::ir::{BinaryOp, UnaryOp};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const INTENT_SCHEMA: &str = "fr-semantic-intent-1";
pub const MAX_LOCATOR_STEPS: usize = 64;
pub const MAX_RESOLVED_TARGETS: usize = 64;
const BODY_BASIS_BYTES: usize = semantic_change::BODY_BASIS_PREFIX.len() + 64;
pub const ROLE_NAMES: &[&str] = &[
    "statement",
    "result",
    "annotation",
    "initializer",
    "assignment-target",
    "assignment-value",
    "condition",
    "then-statement",
    "else-statement",
    "body-statement",
    "finally-statement",
    "iterable",
    "subject",
    "expression",
    "message",
    "callee",
    "argument",
    "receiver",
    "index",
    "left",
    "right",
    "operand",
    "value",
    "fallback",
    "then-expression",
    "else-expression",
    "element",
    "template-part",
    "template-expression",
    "lambda-body",
    "comprehension-element",
    "comprehension-condition",
    "type-expression",
];
pub const OPERATION_NAMES: &[&str] = &[
    "set-int",
    "set-float",
    "set-string",
    "set-bool",
    "set-name",
    "set-field-name",
    "set-keyword-name",
    "set-binary-operator",
    "set-unary-operator",
    "set-template-text",
    "set-comment",
];

#[derive(Args)]
pub struct ApplyOptions {
    #[arg(long, help = "Source-free semantic body JSON, at most 64 KiB.")]
    pub body: PathBuf,
    #[arg(long, help = "Semantic intent JSON, at most 64 KiB.")]
    pub intent: PathBuf,
    #[arg(long, help = "Include the canonical resulting body in the report.")]
    pub canonical: bool,
    #[arg(long, help = "Include the compiled semantic change in the report.")]
    pub compiled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Statement,
    Result,
    Annotation,
    Initializer,
    AssignmentTarget,
    AssignmentValue,
    Condition,
    ThenStatement,
    ElseStatement,
    BodyStatement,
    FinallyStatement,
    Iterable,
    Subject,
    Expression,
    Message,
    Callee,
    Argument,
    Receiver,
    Index,
    Left,
    Right,
    Operand,
    Value,
    Fallback,
    ThenExpression,
    ElseExpression,
    Element,
    TemplatePart,
    TemplateExpression,
    LambdaBody,
    ComprehensionElement,
    ComprehensionCondition,
    TypeExpression,
}

impl Role {
    fn name(self) -> &'static str {
        match self {
            Self::Statement => "statement",
            Self::Result => "result",
            Self::Annotation => "annotation",
            Self::Initializer => "initializer",
            Self::AssignmentTarget => "assignment-target",
            Self::AssignmentValue => "assignment-value",
            Self::Condition => "condition",
            Self::ThenStatement => "then-statement",
            Self::ElseStatement => "else-statement",
            Self::BodyStatement => "body-statement",
            Self::FinallyStatement => "finally-statement",
            Self::Iterable => "iterable",
            Self::Subject => "subject",
            Self::Expression => "expression",
            Self::Message => "message",
            Self::Callee => "callee",
            Self::Argument => "argument",
            Self::Receiver => "receiver",
            Self::Index => "index",
            Self::Left => "left",
            Self::Right => "right",
            Self::Operand => "operand",
            Self::Value => "value",
            Self::Fallback => "fallback",
            Self::ThenExpression => "then-expression",
            Self::ElseExpression => "else-expression",
            Self::Element => "element",
            Self::TemplatePart => "template-part",
            Self::TemplateExpression => "template-expression",
            Self::LambdaBody => "lambda-body",
            Self::ComprehensionElement => "comprehension-element",
            Self::ComprehensionCondition => "comprehension-condition",
            Self::TypeExpression => "type-expression",
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LocatorStep {
    role: Role,
    index: Option<usize>,
    category: Option<NodeCategory>,
    kind: Option<String>,
    label: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentManifest {
    schema: String,
    base: String,
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    SetInt {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetFloat {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetString {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetBool {
        target: Vec<LocatorStep>,
        from: bool,
        to: bool,
    },
    SetName {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetFieldName {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetKeywordName {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetBinaryOperator {
        target: Vec<LocatorStep>,
        from: BinaryOp,
        to: BinaryOp,
    },
    SetUnaryOperator {
        target: Vec<LocatorStep>,
        from: UnaryOp,
        to: UnaryOp,
    },
    SetTemplateText {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
    SetComment {
        target: Vec<LocatorStep>,
        from: String,
        to: String,
    },
}

pub struct AppliedIntent {
    pub body: SemanticBody,
    pub input_basis: String,
    pub result_basis: String,
    pub intent_sha256: String,
    pub change_sha256: String,
    pub operations: Vec<Value>,
    pub compiled: Value,
    pub nodes: usize,
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

fn pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn append(path: &str, suffix: &str) -> String {
    format!("{path}/{suffix}")
}

fn label(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    let kind = object.get("kind")?.as_str()?;
    let contents = object.get("value")?;
    match kind {
        "int" | "float" | "str" | "name" | "text" | "comment" => contents.as_str(),
        "let" | "field" | "keyword" | "local-function" => {
            contents.get("name").and_then(Value::as_str)
        }
        "if-present" | "while-present" | "for-each" | "for-each-indexed" => {
            contents.get("binding").and_then(Value::as_str)
        }
        _ => None,
    }
}

fn relative_role(kind: &str, role: Role) -> Option<&'static str> {
    match (kind, role) {
        ("return", Role::Result) => Some("value"),
        ("let", Role::Annotation) => Some("value/ty"),
        ("let", Role::Initializer) => Some("value/value"),
        ("assign", Role::AssignmentTarget) => Some("value/target"),
        ("assign", Role::AssignmentValue) => Some("value/value"),
        ("tuple-assign", Role::AssignmentValue) => Some("value/value"),
        ("if", Role::Condition) | ("while", Role::Condition) => Some("value/condition"),
        ("if", Role::ThenStatement) | ("if-present", Role::ThenStatement) => Some("value/then"),
        ("if", Role::ElseStatement) | ("if-present", Role::ElseStatement) => {
            Some("value/otherwise")
        }
        ("if-present", Role::Value) | ("while-present", Role::Value) => Some("value/value"),
        ("while", Role::BodyStatement)
        | ("counted-for", Role::BodyStatement)
        | ("for-each-indexed", Role::BodyStatement)
        | ("while-present", Role::BodyStatement)
        | ("for-each", Role::BodyStatement)
        | ("local-function", Role::BodyStatement)
        | ("try", Role::BodyStatement) => Some("value/body"),
        ("defer", Role::BodyStatement)
        | ("err-defer", Role::BodyStatement)
        | ("block", Role::BodyStatement) => Some("value"),
        ("try", Role::FinallyStatement) => Some("value/finally"),
        ("counted-for", Role::Initializer) => Some("value/init"),
        ("counted-for", Role::Condition) => Some("value/condition"),
        ("counted-for", Role::AssignmentValue) => Some("value/update"),
        ("for-each-indexed", Role::Iterable) | ("for-each", Role::Iterable) => {
            Some("value/iterable")
        }
        ("switch", Role::Subject) | ("match-variants", Role::Subject) => Some("value/subject"),
        ("expr", Role::Expression) | ("throw", Role::Expression) => Some("value"),
        ("assert", Role::Condition) => Some("value/condition"),
        ("assert", Role::Message) => Some("value/message"),
        ("break-with", Role::Value) => Some("value/value"),
        ("field", Role::Receiver) | ("index", Role::Receiver) => Some("value/of"),
        ("index", Role::Index) => Some("value/index"),
        ("call", Role::Callee) | ("new", Role::Callee) => Some("value/callee"),
        ("call", Role::Argument) | ("new", Role::Argument) => Some("value/args"),
        ("binary", Role::Left) => Some("value/left"),
        ("binary", Role::Right) => Some("value/right"),
        ("unary", Role::Operand) => Some("value/operand"),
        ("await", Role::Operand) | ("propagate", Role::Operand) => Some("value"),
        ("keyword", Role::Value) => Some("value/value"),
        ("cast", Role::TypeExpression) | ("instance-of", Role::TypeExpression) => Some("value/ty"),
        ("cast", Role::Value) | ("instance-of", Role::Value) => Some("value/value"),
        ("coalesce", Role::Value) => Some("value/value"),
        ("coalesce", Role::Fallback) => Some("value/fallback"),
        ("ternary", Role::Condition) => Some("value/condition"),
        ("ternary", Role::ThenExpression) => Some("value/then"),
        ("ternary", Role::ElseExpression) => Some("value/otherwise"),
        ("tuple", Role::Element) | ("list-lit", Role::Element) | ("set-lit", Role::Element) => {
            Some("value")
        }
        ("template", Role::TemplatePart) => Some("value"),
        ("expr", Role::TemplateExpression) => Some("value"),
        ("lambda", Role::LambdaBody) => Some("value/body"),
        ("comprehension", Role::ComprehensionElement) => Some("value/element"),
        ("comprehension", Role::Iterable) => Some("value/iterable"),
        ("comprehension", Role::ComprehensionCondition) => Some("value/condition"),
        _ => None,
    }
}

fn step_matches(value: &Value, step: &LocatorStep) -> bool {
    let category_matches = step
        .category
        .is_none_or(|expected| semantic_change::category_matches(value, expected));
    let kind_matches = step
        .kind
        .as_deref()
        .is_none_or(|expected| value.get("kind").and_then(Value::as_str) == Some(expected));
    let label_matches = step
        .label
        .as_deref()
        .is_none_or(|expected| label(value) == Some(expected));
    category_matches && kind_matches && label_matches
}

fn resolve(root: &Value, steps: &[LocatorStep]) -> Result<String> {
    ensure!(
        semantic_locator_bounded(steps.len()),
        "semantic intent locator needs 1 through 64 role steps."
    );
    let mut path = String::new();
    for (number, step) in steps.iter().enumerate() {
        let current = root.pointer(&path).with_context(|| missing_path(&path))?;
        let child_path = if path.is_empty() && matches!(step.role, Role::Statement) {
            "/body".to_string()
        } else {
            let kind = current
                .get("kind")
                .and_then(Value::as_str)
                .with_context(|| {
                    format!(
                        "{} starts from a value without a semantic kind.",
                        describe(step, number)
                    )
                })?;
            let relative = relative_role(kind, step.role).with_context(|| {
                format!(
                    "{} is unavailable on semantic kind '{kind}'.",
                    describe(step, number)
                )
            })?;
            relative.split('/').fold(path.clone(), |path, segment| {
                append(&path, &pointer_segment(segment))
            })
        };
        let child = root.pointer(&child_path).with_context(|| {
            format!(
                "{} resolves to a missing optional role.",
                describe(step, number)
            )
        })?;
        ensure!(
            !child.is_null(),
            "{} resolves to a missing optional role.",
            describe(step, number)
        );
        if let Some(items) = child.as_array() {
            let chosen = if let Some(index) = step.index {
                ensure!(
                    index < items.len(),
                    "{} index is out of bounds.",
                    describe(step, number)
                );
                ensure!(
                    step_matches(&items[index], step),
                    "{} indexed node does not match its expected kind or label.",
                    describe(step, number)
                );
                index
            } else {
                ensure!(
                    step.kind.is_some() || step.label.is_some(),
                    "{} needs an index, kind or label to select from a list.",
                    describe(step, number)
                );
                let matches = items
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| step_matches(value, step))
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                ensure!(
                    matches.len() == 1,
                    "{} must resolve one list item; found {}.",
                    describe(step, number),
                    matches.len()
                );
                matches[0]
            };
            path = append(&child_path, &chosen.to_string());
        } else {
            ensure!(
                step.index.is_none(),
                "{} uses an index on a singular role.",
                describe(step, number)
            );
            ensure!(
                step_matches(child, step),
                "{} node does not match its expected kind or label.",
                describe(step, number)
            );
            path = child_path;
        }
    }
    ensure!(
        path.starts_with("/body/"),
        "semantic intent locator must resolve below the body root."
    );
    Ok(path)
}

fn describe(step: &LocatorStep, number: usize) -> String {
    format!(
        "semantic intent locator step {} ('{}')",
        number + 1,
        step.role.name()
    )
}

fn missing_path(path: &str) -> String {
    format!("semantic intent locator reached missing path '{path}'.")
}

fn portable_integer(value: &str) -> bool {
    value == "0"
        || value
            .as_bytes()
            .first()
            .is_some_and(|first| matches!(first, b'1'..=b'9'))
            && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn portable_float(value: &str) -> bool {
    let Some((whole, fraction)) = value.split_once('.') else {
        return false;
    };
    (whole == "0"
        || whole
            .as_bytes()
            .first()
            .is_some_and(|first| matches!(first, b'1'..=b'9'))
            && whole.bytes().all(|byte| byte.is_ascii_digit()))
        && !fraction.is_empty()
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

fn portable_name(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

struct ScalarEdit {
    code: usize,
    name: &'static str,
    kind: &'static str,
    kind_code: usize,
    category: NodeCategory,
    category_code: usize,
    slot: &'static str,
    from: Value,
    to: Value,
}

fn operation_parts(operation: Operation) -> Result<(Vec<LocatorStep>, ScalarEdit)> {
    let result = match operation {
        Operation::SetInt { target, from, to } => {
            ensure!(
                portable_integer(&from) && portable_integer(&to),
                "set-int needs a portable decimal integer."
            );
            (
                target,
                ScalarEdit {
                    code: 0,
                    name: "set-int",
                    kind: "int",
                    kind_code: 0,
                    category: NodeCategory::Expression,
                    category_code: 2,
                    slot: "/value",
                    from: json!(from),
                    to: json!(to),
                },
            )
        }
        Operation::SetFloat { target, from, to } => {
            ensure!(
                portable_float(&from) && portable_float(&to),
                "set-float needs a portable decimal with a fractional part."
            );
            (
                target,
                ScalarEdit {
                    code: 1,
                    name: "set-float",
                    kind: "float",
                    kind_code: 1,
                    category: NodeCategory::Expression,
                    category_code: 2,
                    slot: "/value",
                    from: json!(from),
                    to: json!(to),
                },
            )
        }
        Operation::SetString { target, from, to } => (
            target,
            ScalarEdit {
                code: 2,
                name: "set-string",
                kind: "str",
                kind_code: 2,
                category: NodeCategory::Expression,
                category_code: 2,
                slot: "/value",
                from: json!(from),
                to: json!(to),
            },
        ),
        Operation::SetBool { target, from, to } => (
            target,
            ScalarEdit {
                code: 3,
                name: "set-bool",
                kind: "bool",
                kind_code: 3,
                category: NodeCategory::Expression,
                category_code: 2,
                slot: "/value",
                from: json!(from),
                to: json!(to),
            },
        ),
        Operation::SetName { target, from, to } => {
            ensure!(
                portable_name(&from) && portable_name(&to),
                "set-name needs a portable identifier."
            );
            (
                target,
                ScalarEdit {
                    code: 4,
                    name: "set-name",
                    kind: "name",
                    kind_code: 5,
                    category: NodeCategory::Expression,
                    category_code: 2,
                    slot: "/value",
                    from: json!(from),
                    to: json!(to),
                },
            )
        }
        Operation::SetFieldName { target, from, to } => {
            ensure!(
                portable_name(&from) && portable_name(&to),
                "set-field-name needs a portable identifier."
            );
            (
                target,
                ScalarEdit {
                    code: 5,
                    name: "set-field-name",
                    kind: "field",
                    kind_code: 6,
                    category: NodeCategory::Expression,
                    category_code: 2,
                    slot: "/value/name",
                    from: json!(from),
                    to: json!(to),
                },
            )
        }
        Operation::SetKeywordName { target, from, to } => {
            ensure!(
                portable_name(&from) && portable_name(&to),
                "set-keyword-name needs a portable identifier."
            );
            (
                target,
                ScalarEdit {
                    code: 6,
                    name: "set-keyword-name",
                    kind: "keyword",
                    kind_code: 13,
                    category: NodeCategory::Expression,
                    category_code: 2,
                    slot: "/value/name",
                    from: json!(from),
                    to: json!(to),
                },
            )
        }
        Operation::SetBinaryOperator { target, from, to } => (
            target,
            ScalarEdit {
                code: 7,
                name: "set-binary-operator",
                kind: "binary",
                kind_code: 9,
                category: NodeCategory::Expression,
                category_code: 2,
                slot: "/value/op",
                from: serde_json::to_value(from)?,
                to: serde_json::to_value(to)?,
            },
        ),
        Operation::SetUnaryOperator { target, from, to } => (
            target,
            ScalarEdit {
                code: 8,
                name: "set-unary-operator",
                kind: "unary",
                kind_code: 10,
                category: NodeCategory::Expression,
                category_code: 2,
                slot: "/value/op",
                from: serde_json::to_value(from)?,
                to: serde_json::to_value(to)?,
            },
        ),
        Operation::SetTemplateText { target, from, to } => (
            target,
            ScalarEdit {
                code: 9,
                name: "set-template-text",
                kind: "text",
                kind_code: 0,
                category: NodeCategory::Template,
                category_code: 3,
                slot: "/value",
                from: json!(from),
                to: json!(to),
            },
        ),
        Operation::SetComment { target, from, to } => (
            target,
            ScalarEdit {
                code: 10,
                name: "set-comment",
                kind: "comment",
                kind_code: 17,
                category: NodeCategory::Statement,
                category_code: 1,
                slot: "/value",
                from: json!(from),
                to: json!(to),
            },
        ),
    };
    ensure!(
        result.1.from != result.1.to,
        "semantic intent operation is a no-op."
    );
    ensure!(
        semantic_intent_operation_allowed(
            result.1.code,
            result.1.category_code,
            result.1.kind_code
        ),
        "semantic intent operation has an invalid category or kind."
    );
    Ok(result)
}

pub fn semantic_intent_admitted(
    schema_matches: bool,
    base_well_formed: bool,
    base_matches: bool,
    source_free: bool,
    operation_count: usize,
) -> bool {
    schema_matches
        && base_well_formed
        && base_matches
        && source_free
        && (1..=semantic_change::MAX_OPERATIONS).contains(&operation_count)
}

pub fn semantic_locator_bounded(steps: usize) -> bool {
    (1..=MAX_LOCATOR_STEPS).contains(&steps)
}

pub fn semantic_intent_operation_allowed(operation: usize, category: usize, kind: usize) -> bool {
    matches!(
        (operation, category, kind),
        (0, 2, 0)
            | (1, 2, 1)
            | (2, 2, 2)
            | (3, 2, 3)
            | (4, 2, 5)
            | (5, 2, 6)
            | (6, 2, 13)
            | (7, 2, 9)
            | (8, 2, 10)
            | (9, 3, 0)
            | (10, 1, 17)
    )
}

pub fn apply(body_input: &str, intent_input: &str) -> Result<AppliedIntent> {
    ensure!(
        intent_input.len() <= semantic_change::MAX_INPUT_BYTES,
        "semantic intent input exceeds 64 KiB."
    );
    let initial = semantic_change::validate_body_input(body_input)?;
    let input_basis = semantic_change::body_basis(&initial.manifest)?;
    let intent_value: Value =
        serde_json::from_str(intent_input).context("semantic intent input must be JSON.")?;
    let source_free = super::semantic_ir::source_free(&intent_value);
    ensure!(source_free, "semantic intent input must be source-free.");
    let manifest: IntentManifest = serde_json::from_value(intent_value)
        .context("semantic intent input must match fr-semantic-intent-1.")?;
    ensure!(
        manifest.schema == INTENT_SCHEMA,
        "semantic intent schema must be fr-semantic-intent-1."
    );
    ensure!(
        manifest.base == input_basis,
        "semantic intent base does not match the input body."
    );
    ensure!(
        (1..=semantic_change::MAX_OPERATIONS).contains(&manifest.operations.len()),
        "semantic intent needs 1 through 64 operations."
    );
    let base_well_formed = manifest
        .base
        .starts_with(semantic_change::BODY_BASIS_PREFIX)
        && manifest.base.len() == BODY_BASIS_BYTES
        && manifest.base[semantic_change::BODY_BASIS_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit());
    ensure!(
        semantic_intent_admitted(
            manifest.schema == INTENT_SCHEMA,
            base_well_formed,
            manifest.base == input_basis,
            source_free,
            manifest.operations.len()
        ),
        "semantic intent admission failed."
    );

    let mut current = serde_json::to_value(&initial.manifest)?;
    let mut compiled_operations = Vec::with_capacity(manifest.operations.len());
    let mut reports = Vec::with_capacity(manifest.operations.len());
    let mut nodes = initial.nodes;
    for (number, operation) in manifest.operations.into_iter().enumerate() {
        let (target, edit) = operation_parts(operation)?;
        let path = resolve(&current, &target)?;
        let before = current
            .pointer(&path)
            .with_context(|| format!("semantic intent target '{path}' does not exist."))?
            .clone();
        ensure!(
            before.get("kind").and_then(Value::as_str) == Some(edit.kind),
            "{} target must be a '{}' node.",
            edit.name,
            edit.kind
        );
        ensure!(
            before.pointer(edit.slot) == Some(&edit.from),
            "{} before-value does not match the selected node.",
            edit.name
        );
        let mut replacement = before.clone();
        *replacement.pointer_mut(edit.slot).with_context(|| {
            format!("{} target has no scalar slot '{}'.", edit.name, edit.slot)
        })? = edit.to.clone();
        *current.pointer_mut(&path).unwrap() = replacement.clone();
        let checked = semantic_change::validate_body_input(&serde_json::to_string(&current)?)
            .with_context(|| {
                format!(
                    "semantic intent operation {} leaves an invalid body.",
                    number + 1
                )
            })?;
        current = serde_json::to_value(&checked.manifest)?;
        nodes = checked.nodes;
        compiled_operations.push(json!({
            "op":"replace", "path":path, "category":edit.category, "value":replacement
        }));
        reports.push(json!({
            "number":number + 1, "op":edit.name, "path":path,
            "category":edit.category, "kind":edit.kind,
            "before_sha256":digest(serde_json::to_vec(&before)?),
            "after_sha256":digest(serde_json::to_vec(current.pointer(&path).unwrap())?)
        }));
    }
    ensure!(
        compiled_operations.len() <= MAX_RESOLVED_TARGETS,
        "semantic intent resolves more than 64 targets."
    );
    let compiled = json!({
        "schema":semantic_change::CHANGE_SCHEMA,
        "base":input_basis,
        "operations":compiled_operations
    });
    let applied = semantic_change::apply(body_input, &serde_json::to_string(&compiled)?)?;
    ensure!(
        serde_json::to_value(&applied.body)? == current,
        "compiled semantic change does not match direct intent interpretation."
    );
    Ok(AppliedIntent {
        body: applied.body,
        input_basis: applied.input_basis,
        result_basis: applied.result_basis,
        intent_sha256: digest(intent_input),
        change_sha256: applied.change_sha256,
        operations: reports,
        compiled,
        nodes,
    })
}

fn input(path: &Path, description: &str) -> Result<String> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "{description} must be a regular file."
    );
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "{description} must be a regular file."
    );
    let mut bytes = Vec::new();
    file.take((semantic_change::MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= semantic_change::MAX_INPUT_BYTES,
        "{description} exceeds 64 KiB."
    );
    String::from_utf8(bytes).with_context(|| format!("{description} must use UTF-8."))
}

pub fn apply_from_files(root: &Path, options: &ApplyOptions) -> Result<Value> {
    let body = input(&root.join(&options.body), "semantic body input")?;
    let intent = input(&root.join(&options.intent), "semantic intent input")?;
    let applied = apply(&body, &intent)?;
    let mut report = json!({
        "schema":"fr-semantic-intent-result-1",
        "semantic_schema":super::semantic_ir::BODY_SCHEMA,
        "intent_schema":INTENT_SCHEMA,
        "change_schema":semantic_change::CHANGE_SCHEMA,
        "valid":true,
        "input_basis":applied.input_basis,
        "result_basis":applied.result_basis,
        "intent_sha256":applied.intent_sha256,
        "change_sha256":applied.change_sha256,
        "operations":applied.operations,
        "statements":applied.body.body.len(),
        "semantic_nodes":applied.nodes,
        "source_free":true,
        "refinement_checked":true
    });
    if options.canonical {
        report["canonical"] = serde_json::to_value(&applied.body)?;
    }
    if options.compiled {
        report["compiled_change"] = applied.compiled;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> String {
        json!({"schema":"fr-semantic-body-1","body":[
            {"kind":"let","value":{"name":"total","ty":null,"value":{
                "kind":"binary","value":{"op":"add","left":{"kind":"name","value":"input"},
                "right":{"kind":"int","value":"1"}}},"mutable":false}},
            {"kind":"return","value":{"kind":"name","value":"total"}}
        ]})
        .to_string()
    }

    fn basis() -> String {
        body_basis(&body())
    }

    fn body_basis(input: &str) -> String {
        let body = semantic_change::validate_body_input(input)
            .unwrap()
            .manifest;
        semantic_change::body_basis(&body).unwrap()
    }

    #[test]
    fn role_intent_compiles_to_the_checked_delta_result() {
        let intent = json!({"schema":INTENT_SCHEMA,"base":basis(),"operations":[{
            "op":"set-int","target":[
                {"role":"statement","kind":"let","label":"total"},
                {"role":"initializer","kind":"binary"},
                {"role":"right","kind":"int"}],
            "from":"1","to":"7"
        }]})
        .to_string();
        let applied = apply(&body(), &intent).unwrap();
        assert!(applied.compiled["operations"][0]["path"] == "/body/0/value/value/value/right");
        assert_eq!(
            serde_json::to_value(applied.body)
                .unwrap()
                .pointer("/body/0/value/value/value/right/value"),
            Some(&json!("7"))
        );
    }

    #[test]
    fn ambiguous_and_stale_locators_refuse() {
        let ambiguous = json!({"schema":INTENT_SCHEMA,"base":basis(),"operations":[{
            "op":"set-name","target":[{"role":"statement","kind":"return"},{"role":"result"}],
            "from":"total","to":"answer"
        }]})
        .to_string();
        assert!(apply(&body(), &ambiguous).is_ok());
        let stale = ambiguous.replace("\"from\":\"total\"", "\"from\":\"missing\"");
        assert!(apply(&body(), &stale).is_err());
        let no_selector = json!({"schema":INTENT_SCHEMA,"base":basis(),"operations":[{
            "op":"set-comment","target":[{"role":"statement"}],"from":"a","to":"b"
        }]})
        .to_string();
        assert!(apply(&body(), &no_selector).is_err());
    }

    #[test]
    fn portable_scalars_and_shape_are_checked() {
        let invalid = json!({"schema":INTENT_SCHEMA,"base":basis(),"operations":[{
            "op":"set-int","target":[{"role":"statement","index":0},{"role":"initializer"},
                {"role":"right"}],"from":"1","to":"0xff"
        }]})
        .to_string();
        assert!(apply(&body(), &invalid).is_err());
        let wrong_kind = invalid
            .replace("\"op\":\"set-int\"", "\"op\":\"set-string\"")
            .replace("\"to\":\"0xff\"", "\"to\":\"ok\"");
        assert!(apply(&body(), &wrong_kind).is_err());

        let bad_category = invalid.replace("\"to\":\"0xff\"", "\"to\":\"2\"").replace(
            "{\"role\":\"right\"}",
            "{\"role\":\"right\",\"category\":\"statement\"}",
        );
        assert!(apply(&body(), &bad_category).is_err());

        for scalar in ["-1", "+1", "01"] {
            let invalid_integer = invalid.replace("0xff", scalar);
            assert!(apply(&body(), &invalid_integer).is_err(), "{scalar}");
        }
    }

    #[test]
    fn every_scalar_operation_is_shape_preserving_and_ordered() {
        let input = json!({"schema":"fr-semantic-body-1","body":[
            {"kind":"comment","value":"before"},
            {"kind":"let","value":{"name":"values","ty":null,"mutable":false,"value":{
                "kind":"tuple","value":[
                    {"kind":"int","value":"1"},
                    {"kind":"float","value":"1.5"},
                    {"kind":"str","value":"old"},
                    {"kind":"bool","value":false},
                    {"kind":"name","value":"old_name"},
                    {"kind":"field","value":{"of":{"kind":"name","value":"item"},"name":"old_field"}},
                    {"kind":"keyword","value":{"name":"old_key","value":{"kind":"name","value":"item"}}},
                    {"kind":"binary","value":{"op":"add","left":{"kind":"int","value":"2"},"right":{"kind":"int","value":"3"}}},
                    {"kind":"unary","value":{"op":"not","operand":{"kind":"bool","value":false}}},
                    {"kind":"template","value":[{"kind":"text","value":"old text"},{"kind":"expr","value":{"kind":"name","value":"item"}}]}
                ]
            }}}
        ]})
        .to_string();
        let element = |index: usize, kind: &str| {
            json!([
                {"role":"statement","index":1,"category":"statement","kind":"let"},
                {"role":"initializer","category":"expression","kind":"tuple"},
                {"role":"element","index":index,"category":"expression","kind":kind}
            ])
        };
        let intent = json!({"schema":INTENT_SCHEMA,"base":body_basis(&input),"operations":[
            {"op":"set-int","target":element(0,"int"),"from":"1","to":"4"},
            {"op":"set-float","target":element(1,"float"),"from":"1.5","to":"2.5"},
            {"op":"set-string","target":element(2,"str"),"from":"old","to":"new"},
            {"op":"set-bool","target":element(3,"bool"),"from":false,"to":true},
            {"op":"set-name","target":element(4,"name"),"from":"old_name","to":"new_name"},
            {"op":"set-field-name","target":element(5,"field"),"from":"old_field","to":"new_field"},
            {"op":"set-keyword-name","target":element(6,"keyword"),"from":"old_key","to":"new_key"},
            {"op":"set-binary-operator","target":element(7,"binary"),"from":"add","to":"mul"},
            {"op":"set-unary-operator","target":element(8,"unary"),"from":"not","to":"neg"},
            {"op":"set-template-text","target":[
                {"role":"statement","index":1,"category":"statement","kind":"let"},
                {"role":"initializer","category":"expression","kind":"tuple"},
                {"role":"element","index":9,"category":"expression","kind":"template"},
                {"role":"template-part","index":0,"category":"template","kind":"text"}],
                "from":"old text","to":"new text"},
            {"op":"set-comment","target":[
                {"role":"statement","index":0,"category":"statement","kind":"comment"}],
                "from":"before","to":"after"}
        ]})
        .to_string();
        let applied = apply(&input, &intent).unwrap();
        assert_eq!(applied.operations.len(), OPERATION_NAMES.len());
        assert_eq!(
            applied.compiled["operations"].as_array().unwrap().len(),
            OPERATION_NAMES.len()
        );
        assert_eq!(applied.operations[0]["op"], "set-int");
        assert_eq!(applied.operations[10]["op"], "set-comment");
        let result = serde_json::to_value(applied.body).unwrap();
        assert_eq!(result.pointer("/body/0/value"), Some(&json!("after")));
        assert_eq!(
            result.pointer("/body/1/value/value/value/9/value/0/value"),
            Some(&json!("new text"))
        );
    }
}
