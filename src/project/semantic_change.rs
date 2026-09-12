use crate::transpile::ir::{Expr, Stmt, TemplatePart, Type};
use anyhow::{ensure, Context, Result};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CHANGE_SCHEMA: &str = "fr-semantic-change-1";
pub const BODY_BASIS_PREFIX: &str = "frsb1:";
pub const MAX_INPUT_BYTES: usize = 65_536;
pub const MAX_OPERATIONS: usize = 64;
pub const MAX_STATEMENTS: usize = 512;
pub const MAX_NODES: usize = 4_096;
const MAX_POINTER_BYTES: usize = 1_024;
const MAX_POINTER_SEGMENTS: usize = 64;

#[derive(Args)]
pub struct ApplyOptions {
    #[arg(long, help = "Source-free semantic body JSON, at most 64 KiB.")]
    pub body: PathBuf,
    #[arg(long, help = "Semantic change JSON, at most 64 KiB.")]
    pub change: PathBuf,
    #[arg(long, help = "Include the canonical resulting body in the report.")]
    pub canonical: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[clap(rename_all = "kebab-case")]
pub enum NodeCategory {
    Type,
    Statement,
    Expression,
    Template,
}

impl NodeCategory {
    fn name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Statement => "statement",
            Self::Expression => "expression",
            Self::Template => "template",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticBody {
    pub schema: String,
    pub body: Vec<Stmt>,
}

pub(super) struct ValidatedBody {
    pub(super) manifest: SemanticBody,
    pub(super) input_sha256: String,
    pub(super) canonical: String,
    pub(super) nodes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeManifest {
    schema: String,
    base: String,
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    Replace {
        path: String,
        category: NodeCategory,
        value: Value,
    },
    InsertStatement {
        path: String,
        index: usize,
        value: Value,
    },
    DeleteStatement {
        path: String,
    },
}

pub struct AppliedChange {
    pub body: SemanticBody,
    pub input_basis: String,
    pub result_basis: String,
    pub change_sha256: String,
    pub operations: Vec<Value>,
    pub nodes: usize,
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

pub(super) fn validate_body_input(input: &str) -> Result<ValidatedBody> {
    ensure!(
        input.len() <= MAX_INPUT_BYTES,
        "semantic body input exceeds 64 KiB."
    );
    let value: Value = serde_json::from_str(input).context("semantic body input must be JSON.")?;
    ensure!(
        super::semantic_ir::source_free(&value),
        "semantic body input must not contain source fields or unsupported nodes."
    );
    let nodes = super::semantic::semantic_nodes(&value);
    let manifest: SemanticBody = serde_json::from_value(value)
        .context("semantic body input must match fr-semantic-body-1.")?;
    ensure!(
        manifest.schema == super::semantic_ir::BODY_SCHEMA,
        "semantic body schema must be fr-semantic-body-1."
    );
    ensure!(
        semantic_change_result_bounded(manifest.body.len(), nodes),
        "semantic body input is limited to 512 statements and 4096 semantic nodes."
    );
    let canonical = serde_json::to_string(&manifest)?;
    Ok(ValidatedBody {
        manifest,
        input_sha256: digest(input),
        canonical,
        nodes,
    })
}

pub fn body_basis(body: &SemanticBody) -> Result<String> {
    Ok(format!(
        "{BODY_BASIS_PREFIX}{}",
        digest(serde_json::to_string(body)?)
    ))
}

pub fn semantic_change_admitted(
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
        && (1..=MAX_OPERATIONS).contains(&operation_count)
}

pub fn semantic_change_result_bounded(statements: usize, nodes: usize) -> bool {
    statements <= MAX_STATEMENTS && nodes <= MAX_NODES
}

pub fn semantic_pointer_bounded(bytes: usize, segments: usize) -> bool {
    (1..=MAX_POINTER_BYTES).contains(&bytes) && (1..=MAX_POINTER_SEGMENTS).contains(&segments)
}

pub fn semantic_statement_index_allowed(operation: usize, statements: usize, index: usize) -> bool {
    match operation {
        0 => index <= statements,
        1 => index < statements,
        _ => false,
    }
}

fn canonical_pointer(path: &str, allow_body: bool) -> bool {
    if !path.starts_with('/')
        || (!allow_body && !path.starts_with("/body/"))
        || (allow_body && path != "/body" && !path.starts_with("/body/"))
    {
        return false;
    }
    let segments = path[1..].split('/').collect::<Vec<_>>();
    if !semantic_pointer_bounded(path.len(), segments.len())
        || segments.iter().any(|part| part.is_empty())
    {
        return false;
    }
    segments.iter().all(|part| {
        let mut chars = part.chars();
        while let Some(character) = chars.next() {
            if character == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return false;
            }
        }
        true
    })
}

fn category_matches(value: &Value, category: NodeCategory) -> bool {
    match category {
        NodeCategory::Type => serde_json::from_value::<Type>(value.clone()).is_ok(),
        NodeCategory::Statement => serde_json::from_value::<Stmt>(value.clone()).is_ok(),
        NodeCategory::Expression => serde_json::from_value::<Expr>(value.clone()).is_ok(),
        NodeCategory::Template => serde_json::from_value::<TemplatePart>(value.clone()).is_ok(),
    }
}

fn category_at(body: &Value, path: &str, category: NodeCategory) -> bool {
    let sentinel = match category {
        NodeCategory::Type => json!({"kind":"unit"}),
        NodeCategory::Statement => json!({"kind":"continue"}),
        NodeCategory::Expression => json!({"kind":"null"}),
        NodeCategory::Template => json!({"kind":"text","value":"fr-category-witness"}),
    };
    let mut witness = body.clone();
    let Some(target) = witness.pointer_mut(path) else {
        return false;
    };
    *target = sentinel;
    canonical_body_value(witness).is_ok()
}

fn canonical_body_value(value: Value) -> Result<(Value, SemanticBody, usize)> {
    let input = serde_json::to_string(&value)?;
    let validated = validate_body_input(&input)?;
    let canonical = serde_json::to_value(&validated.manifest)?;
    Ok((canonical, validated.manifest, validated.nodes))
}

fn apply_replace(
    body: &mut Value,
    path: &str,
    category: NodeCategory,
    replacement: Value,
) -> Result<Value> {
    ensure!(
        canonical_pointer(path, false),
        "semantic replacement path must be a bounded canonical pointer below /body."
    );
    ensure!(
        super::semantic_ir::source_free(&replacement),
        "semantic replacement must be source-free and supported."
    );
    let before = body
        .pointer(path)
        .with_context(|| format!("semantic replacement path '{path}' does not exist."))?;
    ensure!(
        category_at(body, path, category) && category_matches(before, category),
        "semantic replacement target is not a {} node.",
        category.name()
    );
    ensure!(
        category_matches(&replacement, category),
        "semantic replacement value is not a {} node.",
        category.name()
    );
    ensure!(before != &replacement, "semantic replacement is a no-op.");
    let before_sha256 = digest(serde_json::to_vec(before)?);
    *body.pointer_mut(path).unwrap() = replacement;
    let after_sha256 = digest(serde_json::to_vec(body.pointer(path).unwrap())?);
    Ok(json!({
        "op": "replace",
        "path": path,
        "category": category,
        "before_sha256": before_sha256,
        "after_sha256": after_sha256
    }))
}

fn apply_insert(body: &mut Value, path: &str, index: usize, statement: Value) -> Result<Value> {
    ensure!(
        canonical_pointer(path, true),
        "semantic statement-list path must be a bounded canonical pointer at or below /body."
    );
    ensure!(
        super::semantic_ir::source_free(&statement)
            && serde_json::from_value::<Stmt>(statement.clone()).is_ok(),
        "semantic insertion value must be a source-free supported statement."
    );
    let list = body
        .pointer_mut(path)
        .with_context(|| format!("semantic statement-list path '{path}' does not exist."))?
        .as_array_mut()
        .with_context(|| format!("semantic insertion path '{path}' is not a statement list."))?;
    ensure!(
        semantic_statement_index_allowed(0, list.len(), index),
        "semantic insertion index is out of bounds."
    );
    list.insert(index, statement);
    Ok(json!({"op":"insert-statement","path":path,"index":index}))
}

fn array_index(segment: &str) -> Option<usize> {
    if segment.is_empty() || segment.len() > 1 && segment.starts_with('0') {
        return None;
    }
    segment.parse().ok()
}

fn apply_delete(body: &mut Value, path: &str) -> Result<Value> {
    ensure!(
        canonical_pointer(path, false),
        "semantic deletion path must be a bounded canonical pointer below /body."
    );
    let (parent_path, segment) = path
        .rsplit_once('/')
        .context("semantic deletion path must select one statement.")?;
    let index = array_index(segment)
        .context("semantic deletion path must end in a canonical array index.")?;
    let list = body
        .pointer_mut(parent_path)
        .with_context(|| format!("semantic deletion parent '{parent_path}' does not exist."))?
        .as_array_mut()
        .with_context(|| {
            format!("semantic deletion parent '{parent_path}' is not a statement list.")
        })?;
    ensure!(
        semantic_statement_index_allowed(1, list.len(), index),
        "semantic deletion index is out of bounds."
    );
    ensure!(
        serde_json::from_value::<Stmt>(list[index].clone()).is_ok(),
        "semantic deletion target is not a statement."
    );
    let removed = list.remove(index);
    Ok(json!({
        "op":"delete-statement",
        "path":path,
        "before_sha256":digest(serde_json::to_vec(&removed)?)
    }))
}

pub fn apply(body_input: &str, change_input: &str) -> Result<AppliedChange> {
    ensure!(
        change_input.len() <= MAX_INPUT_BYTES,
        "semantic change input exceeds 64 KiB."
    );
    let validated = validate_body_input(body_input)?;
    let input_basis = body_basis(&validated.manifest)?;
    let change_value: Value =
        serde_json::from_str(change_input).context("semantic change input must be JSON.")?;
    let source_free = super::semantic_ir::source_free(&change_value);
    ensure!(
        source_free,
        "semantic change input must not contain source fields or unsupported nodes."
    );
    let change: ChangeManifest = serde_json::from_value(change_value)
        .context("semantic change input must match fr-semantic-change-1.")?;
    ensure!(
        change.schema == CHANGE_SCHEMA,
        "semantic change schema must be fr-semantic-change-1."
    );
    let base_well_formed = change.base.starts_with(BODY_BASIS_PREFIX)
        && change.base.len() == BODY_BASIS_PREFIX.len() + 64
        && change.base[BODY_BASIS_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit());
    ensure!(
        base_well_formed,
        "semantic change base must be an frsb1 SHA-256 identity."
    );
    ensure!(
        change.base == input_basis,
        "semantic change base does not match the input body."
    );
    ensure!(
        (1..=MAX_OPERATIONS).contains(&change.operations.len()),
        "semantic change needs 1 through 64 operations."
    );
    ensure!(
        semantic_change_admitted(
            change.schema == CHANGE_SCHEMA,
            base_well_formed,
            change.base == input_basis,
            source_free,
            change.operations.len()
        ),
        "semantic change admission failed."
    );

    let mut current = serde_json::to_value(&validated.manifest)?;
    let mut reports = Vec::with_capacity(change.operations.len());
    let mut final_body = validated.manifest;
    let mut nodes = validated.nodes;
    for (index, operation) in change.operations.into_iter().enumerate() {
        let mut report = match operation {
            Operation::Replace {
                path,
                category,
                value,
            } => apply_replace(&mut current, &path, category, value)?,
            Operation::InsertStatement { path, index, value } => {
                apply_insert(&mut current, &path, index, value)?
            }
            Operation::DeleteStatement { path } => apply_delete(&mut current, &path)?,
        };
        let (canonical, body, count) = canonical_body_value(current)
            .with_context(|| format!("semantic operation {} leaves an invalid body.", index + 1))?;
        report["number"] = json!(index + 1);
        report["result_basis"] = json!(body_basis(&body)?);
        reports.push(report);
        current = canonical;
        final_body = body;
        nodes = count;
    }
    let result_basis = body_basis(&final_body)?;
    ensure!(input_basis != result_basis, "semantic change is a no-op.");
    Ok(AppliedChange {
        body: final_body,
        input_basis,
        result_basis,
        change_sha256: digest(change_input),
        operations: reports,
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
    file.take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_INPUT_BYTES,
        "{description} exceeds 64 KiB."
    );
    String::from_utf8(bytes).with_context(|| format!("{description} must use UTF-8."))
}

pub fn apply_from_files(root: &Path, options: &ApplyOptions) -> Result<Value> {
    let body = input(&root.join(&options.body), "semantic body input")?;
    let change = input(&root.join(&options.change), "semantic change input")?;
    let applied = apply(&body, &change)?;
    let mut report = json!({
        "schema": "fr-semantic-change-result-1",
        "semantic_schema": super::semantic_ir::BODY_SCHEMA,
        "change_schema": CHANGE_SCHEMA,
        "valid": true,
        "input_basis": applied.input_basis,
        "result_basis": applied.result_basis,
        "change_sha256": applied.change_sha256,
        "operations": applied.operations,
        "statements": applied.body.body.len(),
        "semantic_nodes": applied.nodes,
        "source_free": true
    });
    if options.canonical {
        report["canonical"] = serde_json::to_value(&applied.body)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> String {
        r#"{"schema":"fr-semantic-body-1","body":[{"kind":"return","value":{"kind":"name","value":"left"}}]}"#.into()
    }

    fn change(base: &str, operations: Value) -> String {
        json!({"schema":CHANGE_SCHEMA,"base":base,"operations":operations}).to_string()
    }

    #[test]
    fn replacement_is_typed_and_basis_bound() {
        let validated = validate_body_input(&body()).unwrap();
        let basis = body_basis(&validated.manifest).unwrap();
        let input = change(
            &basis,
            json!([{"op":"replace","path":"/body/0/value","category":"expression",
                "value":{"kind":"name","value":"right"}}]),
        );
        let applied = apply(&body(), &input).unwrap();
        assert_ne!(applied.input_basis, applied.result_basis);
        assert_eq!(applied.body.body.len(), 1);
        let stale = change(
            &format!("{BODY_BASIS_PREFIX}{}", "0".repeat(64)),
            json!([{"op":"delete-statement","path":"/body/0"}]),
        );
        assert!(apply(&body(), &stale).is_err());
    }

    #[test]
    fn every_intermediate_body_must_remain_valid() {
        let validated = validate_body_input(&body()).unwrap();
        let basis = body_basis(&validated.manifest).unwrap();
        let wrong_category = change(
            &basis,
            json!([{"op":"replace","path":"/body/0/value","category":"statement",
                "value":{"kind":"comment","value":"wrong"}}]),
        );
        assert!(apply(&body(), &wrong_category).is_err());
        let wrong_list = change(
            &basis,
            json!([{"op":"insert-statement","path":"/body/0/value","index":0,
                "value":{"kind":"comment","value":"wrong"}}]),
        );
        assert!(apply(&body(), &wrong_list).is_err());
    }

    #[test]
    fn structural_context_separates_identically_encoded_variants() {
        let statement_body = r#"{"schema":"fr-semantic-body-1","body":[{"kind":"expr","value":{"kind":"name","value":"left"}}]}"#;
        let validated = validate_body_input(statement_body).unwrap();
        let basis = body_basis(&validated.manifest).unwrap();
        let wrong = change(
            &basis,
            json!([{"op":"replace","path":"/body/0","category":"template",
                "value":{"kind":"expr","value":{"kind":"name","value":"right"}}}]),
        );
        assert!(apply(statement_body, &wrong).is_err());
    }
}
