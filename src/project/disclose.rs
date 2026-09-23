use super::{bounded_text, hash, source_slice_length, AgentProfile, Project};
use crate::span::{LineIndex, Span};
use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SCHEMA: &str = "fr-progressive-disclosure-1";
const TREE_SCHEMA: &str = "fr-semantic-merkle-1";
const OBJECT_SCHEMA: &str = "fr-merkle-object-1";
const PROOF_SCHEMA: &str = "fr-merkle-inclusion-1";
pub(crate) const EDIT_SCHEMA: &str = "fr-disclosed-edit-1";
pub(crate) const IR_EDIT_SCHEMA: &str = "fr-disclosed-ir-edit-1";
const INLINE_SCALAR_BYTES: usize = 128;
const MIN_TOKEN_LIMIT: usize = 1_024;

#[doc = "Admission policy for a completed agent-context materialization."]
pub fn context_materialization_admitted(
    calls: usize,
    call_limit: usize,
    session_matches: bool,
    complete: bool,
    digest_matches: bool,
) -> bool {
    (1..=64).contains(&call_limit)
        && calls <= call_limit
        && session_matches
        && complete
        && digest_matches
}

#[doc = "Admission policy for a completed declarative agent intent."]
// Keep this flat signature aligned with the Python and Lean executable corpora.
#[allow(clippy::too_many_arguments)]
pub fn agent_intent_admitted(
    needs: usize,
    sections: usize,
    calls: usize,
    call_limit: usize,
    packet_bytes: usize,
    packet_limit: usize,
    target_matches: bool,
    session_matches: bool,
    complete: bool,
) -> bool {
    (1..=32).contains(&needs)
        && (1..=8).contains(&sections)
        && sections <= needs
        && (1..=512).contains(&call_limit)
        && calls <= call_limit
        && (1_024..=65_536).contains(&packet_limit)
        && packet_bytes <= packet_limit
        && target_matches
        && session_matches
        && complete
}

#[doc = "Admission policy for one generated Merkle object pack."]
pub fn object_store_admitted(
    objects: usize,
    encoded_bytes: usize,
    digest_matches: bool,
    records_canonical: bool,
    root_present: bool,
) -> bool {
    (1..=65_536).contains(&objects)
        && encoded_bytes <= 67_108_864
        && digest_matches
        && records_canonical
        && root_present
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditRequest {
    pub(crate) edit: String,
    pub(crate) to: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IrEditRequest {
    pub(crate) edit: String,
    pub(crate) value: Option<Value>,
}

pub(crate) fn edit_id_well_formed(value: &str) -> bool {
    value.strip_prefix("frde1:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub(crate) fn ir_edit_id_well_formed(value: &str) -> bool {
    value.strip_prefix("frdi1:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub(crate) fn scalar_commitment(value: &Value) -> Result<Value> {
    let bytes = serde_json::to_vec(value)?;
    Ok(json!({
        "algorithm": "sha256-tagged-canonical-json-tree",
        "digest": merkle(value)?,
        "serialized_bytes": bytes.len()
    }))
}

pub(crate) fn scalar_is_inline(value: &Value) -> Result<bool> {
    Ok(serde_json::to_vec(value)?.len() <= INLINE_SCALAR_BYTES)
}

#[derive(Args)]
pub struct Options {
    #[arg(
        help = "Full revision-bound project handle; semantic and evidence views require a declaration."
    )]
    target: String,
    #[arg(
        long,
        help = "Opaque hole ID returned by an earlier disclosure response."
    )]
    reveal: Option<String>,
    #[arg(
        long,
        requires = "reveal",
        help = "Opaque continuation cursor returned with the hole."
    )]
    cursor: Option<String>,
    #[arg(
        long,
        default_value_t = 4_096,
        help = "Conservative model-token upper bound for the complete JSON line."
    )]
    token_limit: usize,
    #[arg(long, value_enum, default_value = "compact")]
    pub(super) profile: AgentProfile,
    #[arg(
        long,
        value_enum,
        default_value = "semantic",
        help = "Reveal semantic IR, analysis evidence or cross-stack project objects."
    )]
    view: DisclosureView,
    #[arg(
        long,
        default_value_t = 3,
        help = "Hierarchy, call-trace and value-flow depth for the evidence view (0..8)."
    )]
    depth: usize,
    #[arg(
        long,
        help = "Include standalone Merkle paths for verification and protocol testing."
    )]
    proofs: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum DisclosureView {
    Semantic,
    Evidence,
    Project,
    Application,
}

pub fn disclosure_budget_admitted(
    serialized_bytes: usize,
    token_limit: usize,
    profile: usize,
) -> bool {
    let maximum = match profile {
        0 => 4_096,
        1 => 16_384,
        _ => return false,
    };
    (MIN_TOKEN_LIMIT..=maximum).contains(&token_limit) && serialized_bytes <= token_limit
}

pub fn disclosure_transition_allowed(kind: usize, offset: usize, total: usize) -> bool {
    match kind {
        0 => offset <= total,
        1 => offset <= total,
        _ => false,
    }
}

pub fn disclosure_frontier_after(hidden: u64, children: u64) -> Option<u64> {
    hidden.checked_sub(1)?.checked_add(children)
}

pub fn disclosure_proof_step_allowed(width: usize, index: usize, side: usize) -> bool {
    width > 1
        && index < width
        && match side {
            0 => index % 2 == 1,
            1 => index.is_multiple_of(2) && index + 1 < width,
            2 => index.is_multiple_of(2) && index + 1 == width,
            _ => false,
        }
}

pub fn disclosure_proof_parent(width: usize, index: usize) -> Option<(usize, usize)> {
    (width > 1 && index < width).then_some((width / 2 + width % 2, index / 2))
}

pub fn disclosure_view_admitted(view: usize, depth: usize) -> bool {
    view == 0 || ((view == 1 || view == 2 || view == 3) && depth <= 8)
}

#[derive(Clone)]
struct View {
    target: String,
    semantic_basis: String,
    model: Value,
    semantic_root: String,
    object_root: String,
    source_root: String,
    root: String,
    basis: String,
    source: String,
    kind: DisclosureView,
    edits: Vec<DisclosedEdit>,
    ir_edits: Vec<DisclosedIrEdit>,
}

#[derive(Clone)]
struct DisclosedEdit {
    id: String,
    pointer: String,
    operation: String,
    from: Value,
}

#[derive(Clone)]
struct DisclosedIrEdit {
    id: String,
    pointer: String,
    target: super::semantic_change::BodyStructuralTarget,
}

struct ChildReveal<'a> {
    pointer: &'a str,
    node: &'a Value,
    children: Vec<(String, &'a Value)>,
    start: usize,
}

struct StringReveal<'a> {
    pointer: &'a str,
    text: &'a str,
    start: usize,
}

pub(crate) fn merkle(value: &Value) -> Result<String> {
    match value {
        Value::Null => hash((TREE_SCHEMA, "null")),
        Value::Bool(value) => hash((TREE_SCHEMA, "bool", value)),
        Value::Number(value) => hash((TREE_SCHEMA, "number", value.to_string())),
        Value::String(value) => hash((TREE_SCHEMA, "string", value)),
        Value::Array(values) => {
            let children = values.iter().map(merkle).collect::<Result<Vec<_>>>()?;
            hash((TREE_SCHEMA, "array", children))
        }
        Value::Object(object) => {
            let mut children = object
                .iter()
                .map(|(key, value)| Ok((key, merkle(value)?)))
                .collect::<Result<Vec<_>>>()?;
            children.sort_unstable_by(|left, right| left.0.cmp(right.0));
            hash((TREE_SCHEMA, "object", children))
        }
    }
}

/// Address a JSON tree and support logarithmic inclusion paths.
pub fn object_merkle(value: &Value) -> Result<String> {
    match value {
        Value::Null => hash((OBJECT_SCHEMA, "null")),
        Value::Bool(value) => hash((OBJECT_SCHEMA, "bool", value)),
        Value::Number(value) => hash((OBJECT_SCHEMA, "number", value.to_string())),
        Value::String(value) => hash((OBJECT_SCHEMA, "string", value)),
        Value::Array(values) => {
            let leaves = values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    hash((OBJECT_SCHEMA, "array-entry", index, object_merkle(value)?))
                })
                .collect::<Result<Vec<_>>>()?;
            hash((OBJECT_SCHEMA, "array", values.len(), binary_root(leaves)?))
        }
        Value::Object(object) => {
            let mut children = object.iter().collect::<Vec<_>>();
            children.sort_unstable_by(|left, right| left.0.cmp(right.0));
            let leaves = children
                .iter()
                .enumerate()
                .map(|(index, (key, value))| {
                    hash((
                        OBJECT_SCHEMA,
                        "object-entry",
                        index,
                        key,
                        object_merkle(value)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            hash((
                OBJECT_SCHEMA,
                "object",
                children.len(),
                binary_root(leaves)?,
            ))
        }
    }
}

fn binary_root(mut level: Vec<String>) -> Result<String> {
    if level.is_empty() {
        return hash((OBJECT_SCHEMA, "empty"));
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            next.push(match pair {
                [left, right] => hash((OBJECT_SCHEMA, "pair", left, right))?,
                [only] => only.clone(),
                _ => unreachable!(),
            });
        }
        level = next;
    }
    Ok(level.pop().unwrap())
}

fn binary_branch(mut level: Vec<String>, mut index: usize) -> Result<Vec<Value>> {
    let mut branch = Vec::new();
    while level.len() > 1 {
        let side = if index % 2 == 1 {
            0
        } else if index + 1 < level.len() {
            1
        } else {
            2
        };
        ensure!(
            disclosure_proof_step_allowed(level.len(), index, side),
            "invalid Merkle proof branch position."
        );
        if side == 0 {
            branch.push(json!({"side": "left", "digest": level[index - 1]}));
        } else if side == 1 {
            branch.push(json!({"side": "right", "digest": level[index + 1]}));
        } else {
            branch.push(json!({"side": "promote"}));
        }
        let parent = disclosure_proof_parent(level.len(), index)
            .context("Merkle proof branch has no parent")?;
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            next.push(match pair {
                [left, right] => hash((OBJECT_SCHEMA, "pair", left, right))?,
                [only] => only.clone(),
                _ => unreachable!(),
            });
        }
        level = next;
        ensure!(
            parent == (level.len(), index / 2),
            "Merkle proof parent disagrees with the constructed tree."
        );
        index = parent.1;
    }
    Ok(branch)
}

fn inclusion_proof(value: &Value, wanted: &str, pointer: &str) -> Result<Option<Vec<Value>>> {
    if pointer == wanted {
        return Ok(Some(Vec::new()));
    }
    match value {
        Value::Array(values) => {
            let digests = values
                .iter()
                .map(object_merkle)
                .collect::<Result<Vec<_>>>()?;
            for (index, child) in values.iter().enumerate() {
                let child_pointer = pointer_child(pointer, &index.to_string());
                if let Some(mut path) = inclusion_proof(child, wanted, &child_pointer)? {
                    let leaves = digests
                        .iter()
                        .enumerate()
                        .map(|(position, digest)| {
                            hash((OBJECT_SCHEMA, "array-entry", position, digest))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    path.push(json!({
                        "container": "array", "index": index, "length": values.len(),
                        "branch": binary_branch(leaves, index)?
                    }));
                    return Ok(Some(path));
                }
            }
        }
        Value::Object(object) => {
            let mut children = object.iter().collect::<Vec<_>>();
            children.sort_unstable_by(|left, right| left.0.cmp(right.0));
            let digests = children
                .iter()
                .map(|(_, value)| object_merkle(value))
                .collect::<Result<Vec<_>>>()?;
            for (index, (key, child)) in children.iter().enumerate() {
                let child_pointer = pointer_child(pointer, key);
                if let Some(mut path) = inclusion_proof(child, wanted, &child_pointer)? {
                    let leaves = children
                        .iter()
                        .enumerate()
                        .map(|(position, (key, _))| {
                            hash((
                                OBJECT_SCHEMA,
                                "object-entry",
                                position,
                                key,
                                &digests[position],
                            ))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    path.push(json!({
                        "container": "object", "key": key, "index": index,
                        "length": children.len(), "branch": binary_branch(leaves, index)?
                    }));
                    return Ok(Some(path));
                }
            }
        }
        _ => {}
    }
    Ok(None)
}

fn proof(view: &View, pointer: &str, value: &Value) -> Result<Value> {
    let path = inclusion_proof(&view.model, pointer, "/model")?
        .context("revealed semantic pointer is outside its committed proof tree.")?;
    Ok(json!({
        "schema": PROOF_SCHEMA,
        "algorithm": "sha256-tagged-binary-json-tree",
        "leaf": object_merkle(value)?,
        "root": view.object_root,
        "path": path
    }))
}

fn attach_proof(
    report: &mut Value,
    options: &Options,
    view: &View,
    pointer: &str,
    value: &Value,
) -> Result<()> {
    if options.proofs {
        report["revealed"]["proof"] = proof(view, pointer, value)?;
    }
    Ok(())
}

fn pointer_child(pointer: &str, key: &str) -> String {
    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn summary(value: &Value) -> Value {
    match value {
        Value::Object(object) => json!({
            "kind": object.get("kind").and_then(Value::as_str).map(|value| bounded_text(value, 80)),
            "name": object.get("name").and_then(Value::as_str).map(|value| bounded_text(value, 160)),
            "fields": object.len()
        }),
        Value::Array(values) => json!({"items": values.len()}),
        Value::String(value) => json!({"utf8_bytes": value.len()}),
        _ => json!({"value_kind": value_kind(value)}),
    }
}

fn semantic_shortcuts(view: &View, options: &Options) -> Result<Vec<Value>> {
    fn collect(
        view: &View,
        options: &Options,
        value: &Value,
        pointer: &str,
        out: &mut Vec<Value>,
    ) -> Result<()> {
        if out.len() >= options.profile.row_limit() {
            return Ok(());
        }
        match value {
            Value::Object(object) => {
                if object.get("kind").and_then(Value::as_str).is_some()
                    || object.get("name").and_then(Value::as_str).is_some()
                {
                    let digest = merkle(value)?;
                    let id = hole_id(&view.basis, pointer, &digest)?;
                    let descendant = format!("{pointer}/");
                    let editable_scalars = view
                        .edits
                        .iter()
                        .filter(|edit| {
                            edit.pointer == pointer || edit.pointer.starts_with(&descendant)
                        })
                        .count();
                    let editable_ir = view
                        .ir_edits
                        .iter()
                        .filter(|edit| {
                            edit.pointer == pointer || edit.pointer.starts_with(&descendant)
                        })
                        .count();
                    out.push(json!({
                        "address": format!("{}#{pointer}", view.semantic_basis),
                        "kind": object.get("kind").and_then(Value::as_str).map(|value| bounded_text(value, 80)),
                        "name": object.get("name").and_then(Value::as_str).map(|value| bounded_text(value, 160)),
                        "editable_scalars": editable_scalars,
                        "editable_ir": editable_ir,
                        "hole": id,
                        "reveal": {"arguments": arguments(options, &id, None)}
                    }));
                }
                for (key, child) in object {
                    collect(view, options, child, &pointer_child(pointer, key), out)?;
                }
            }
            Value::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    collect(
                        view,
                        options,
                        child,
                        &pointer_child(pointer, &index.to_string()),
                        out,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut rows = Vec::new();
    collect(view, options, &view.model, "/model", &mut rows)?;
    Ok(rows)
}

fn evidence_shortcuts(view: &View, options: &Options) -> Result<Vec<Value>> {
    fn descendants(value: &Value) -> usize {
        match value {
            Value::Array(values) => values.len() + values.iter().map(descendants).sum::<usize>(),
            Value::Object(object) => object.len() + object.values().map(descendants).sum::<usize>(),
            _ => 0,
        }
    }
    let object = view
        .model
        .as_object()
        .context("project evidence root is not an object")?;
    ["code_map", "call_traces", "impact", "sources_and_sinks"]
        .into_iter()
        .filter_map(|name| object.get(name).map(|value| (name, value)))
        .map(|(name, value)| {
            let pointer = pointer_child("/model", name);
            let digest = merkle(value)?;
            let id = hole_id(&view.basis, &pointer, &digest)?;
            Ok(json!({
                "domain": name,
                "address": format!("{}#{pointer}", view.semantic_basis),
                "value_kind": value_kind(value),
                "hidden_descendants": descendants(value),
                "hole": id,
                "reveal": {"arguments": arguments(options, &id, None)}
            }))
        })
        .collect()
}

fn evidence_catalog(view: &View) -> Result<Vec<Value>> {
    fn descendants(value: &Value) -> usize {
        match value {
            Value::Array(values) => values.len() + values.iter().map(descendants).sum::<usize>(),
            Value::Object(object) => object.len() + object.values().map(descendants).sum::<usize>(),
            _ => 0,
        }
    }
    let object = view
        .model
        .as_object()
        .context("project evidence root is not an object")?;
    Ok(["code_map", "call_traces", "impact", "sources_and_sinks"]
        .into_iter()
        .filter_map(|name| object.get(name).map(|value| (name, value)))
        .map(|(name, value)| json!({"domain": name, "hidden_descendants": descendants(value)}))
        .collect())
}

const PROJECT_DOMAINS: [&str; 5] = [
    "technologies",
    "packages",
    "applications",
    "styles",
    "documents_and_diagrams",
];

fn project_shortcuts(view: &View, options: &Options) -> Result<Vec<Value>> {
    let object = view
        .model
        .as_object()
        .context("cross-stack project root is not an object")?;
    let domains: &[&str] = if view.kind == DisclosureView::Application {
        &["applications", "omissions"]
    } else {
        &PROJECT_DOMAINS
    };
    domains
        .iter()
        .copied()
        .filter_map(|name| object.get(name).map(|value| (name, value)))
        .take(options.profile.row_limit())
        .map(|(name, value)| {
            let pointer = pointer_child("/model", name);
            let digest = merkle(value)?;
            let id = hole_id(&view.basis, &pointer, &digest)?;
            Ok(json!({
                "domain": name,
                "address": format!("{}#{pointer}", view.semantic_basis),
                "value_kind": value_kind(value),
                "object_digest": object_merkle(value)?,
                "hole": id,
                "reveal": {"arguments": arguments(options, &id, None)}
            }))
        })
        .collect()
}

fn project_catalog(view: &View) -> Result<Vec<Value>> {
    let object = view
        .model
        .as_object()
        .context("cross-stack project root is not an object")?;
    PROJECT_DOMAINS
        .into_iter()
        .filter_map(|name| object.get(name).map(|value| (name, value)))
        .map(|(name, value)| Ok(json!({"domain": name, "object_digest": object_merkle(value)?})))
        .collect()
}

fn hole_id(basis: &str, pointer: &str, digest: &str) -> Result<String> {
    Ok(format!("frh1:{}", hash((SCHEMA, basis, pointer, digest))?))
}

fn source_hole_id(basis: &str, digest: &str, offset: usize) -> Result<String> {
    Ok(format!("frs1:{}", hash((SCHEMA, basis, digest, offset))?))
}

pub(crate) fn disclosed_edit_id(
    revision: &str,
    target: &str,
    body_basis: &str,
    scalar: &super::semantic_intent::BodyScalarTarget,
) -> Result<String> {
    Ok(format!(
        "frde1:{}",
        hash((
            EDIT_SCHEMA,
            revision,
            target,
            body_basis,
            &scalar.scalar_pointer,
            &scalar.operation,
            &scalar.from,
            &scalar.target
        ))?
    ))
}

pub(crate) fn disclosed_ir_edit_id(
    revision: &str,
    target: &str,
    body_basis: &str,
    structural: &super::semantic_change::BodyStructuralTarget,
) -> Result<String> {
    Ok(format!(
        "frdi1:{}",
        hash((
            IR_EDIT_SCHEMA,
            revision,
            target,
            body_basis,
            &structural.address_pointer,
            &structural.path,
            structural.index,
            structural.category,
            structural.operation,
            structural.placement,
            merkle(&structural.current)?
        ))?
    ))
}

fn edit_descriptor(view: &View, pointer: &str) -> Result<Option<Value>> {
    let Some(edit) = view.edits.iter().find(|edit| edit.pointer == pointer) else {
        return Ok(None);
    };
    let mut descriptor = json!({
        "schema": EDIT_SCHEMA,
        "id": edit.id,
        "operation": edit.operation,
        "preview_template": {
            "arguments": [
                "author", "edit-body-disclosed", view.target,
                "--edit", edit.id, "--to", "<NEW_VALUE>"
            ],
            "replace": "<NEW_VALUE>"
        }
    });
    if scalar_is_inline(&edit.from)? {
        descriptor["from"] = edit.from.clone();
    } else {
        descriptor["from_commitment"] = scalar_commitment(&edit.from)?;
    }
    Ok(Some(descriptor))
}

fn ir_edit_descriptors(view: &View, profile: AgentProfile, pointer: &str) -> Result<Vec<Value>> {
    if profile == AgentProfile::Compact {
        return Ok(Vec::new());
    }
    view.ir_edits
        .iter()
        .filter(|edit| edit.pointer == pointer)
        .map(|edit| {
            let mut arguments = vec![
                "author",
                "edit-body-disclosed-ir",
                &view.target,
                "--edit",
                &edit.id,
            ];
            if edit.target.operation.value_required() {
                arguments.extend(["--from", "<IR_NODE_JSON>"]);
            }
            let mut descriptor = json!({
                "schema": IR_EDIT_SCHEMA,
                "id": edit.id,
                "operation": edit.target.operation.name(),
                "placement": edit.target.placement,
                "accepts": edit.target.category,
                "current_commitment": scalar_commitment(&edit.target.current)?,
                "preview_template": {
                    "arguments": arguments,
                    "replace": if edit.target.operation.value_required() {
                        json!("<IR_NODE_JSON>")
                    } else {
                        Value::Null
                    }
                }
            });
            if edit.target.operation.value_required() {
                descriptor["schema_action"] = json!({
                    "arguments": ["author", "semantic-schema", edit.target.category.name()]
                });
            }
            Ok(descriptor)
        })
        .collect()
}

fn arguments(options: &Options, hole: &str, cursor: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "project".into(),
        "disclose".into(),
        options.target.clone(),
        "--reveal".into(),
        hole.into(),
        "--token-limit".into(),
        options.token_limit.to_string(),
    ];
    if options.profile == AgentProfile::Expanded {
        args.extend(["--profile".into(), "expanded".into()]);
    }
    if matches!(
        options.view,
        DisclosureView::Evidence | DisclosureView::Project | DisclosureView::Application
    ) {
        args.extend([
            "--view".into(),
            match options.view {
                DisclosureView::Evidence => "evidence".into(),
                DisclosureView::Project => "project".into(),
                DisclosureView::Application => "application".into(),
                DisclosureView::Semantic => unreachable!(),
            },
            "--depth".into(),
            options.depth.to_string(),
        ]);
    }
    if options.proofs {
        args.push("--proofs".into());
    }
    if let Some(cursor) = cursor {
        args.extend(["--cursor".into(), cursor.into()]);
    }
    args
}

fn tree_domain(view: &View) -> &'static str {
    match view.kind {
        DisclosureView::Semantic => "semantic-ir",
        DisclosureView::Evidence => "project-evidence",
        DisclosureView::Project => "cross-stack-project",
        DisclosureView::Application => "application-ir",
    }
}

fn semantic_hole(view: &View, options: &Options, pointer: &str, value: &Value) -> Result<Value> {
    let digest = merkle(value)?;
    let id = hole_id(&view.basis, pointer, &digest)?;
    Ok(json!({
        "id": id,
        "domain": tree_domain(view),
        "address": format!("{}#{pointer}", view.semantic_basis),
        "digest": digest,
        "object_digest": object_merkle(value)?,
        "value_kind": value_kind(value),
        "summary": summary(value),
        "subtree_bytes": serde_json::to_vec(value)?.len(),
        "reveal": {"arguments": arguments(options, &id, None)}
    }))
}

fn source_hole(
    view: &View,
    options: &Options,
    offset: usize,
    cursor: Option<&str>,
) -> Result<Value> {
    let id = source_hole_id(&view.basis, &view.source_root, offset)?;
    Ok(json!({
        "id": id,
        "domain": "exact-source",
        "digest": view.source_root,
        "offset": offset,
        "remaining_bytes": view.source.len() - offset,
        "reveal": {"arguments": arguments(options, &id, cursor)}
    }))
}

fn cursor(view: &View, options: &Options, hole: &str, offset: usize) -> Result<String> {
    Ok(format!(
        "frdc1:{}:{offset}",
        hash((
            SCHEMA,
            &view.basis,
            hole,
            options.profile,
            options.token_limit,
            options.proofs,
            offset
        ))?
    ))
}

fn cursor_offset(view: &View, options: &Options, hole: &str) -> Result<usize> {
    let Some(cursor_value) = options.cursor.as_deref() else {
        return Ok(0);
    };
    let (prefix, offset) = cursor_value
        .rsplit_once(':')
        .context("invalid disclosure cursor")?;
    let offset = offset
        .parse::<usize>()
        .context("invalid disclosure cursor offset")?;
    ensure!(
        prefix
            == format!(
                "frdc1:{}",
                hash((
                    SCHEMA,
                    &view.basis,
                    hole,
                    options.profile,
                    options.token_limit,
                    options.proofs,
                    offset
                ))?
            ),
        "stale or conflicting disclosure cursor. Use an exact returned reveal action."
    );
    Ok(offset)
}

fn find_hole<'a>(
    view: &View,
    wanted: &str,
    value: &'a Value,
    pointer: &str,
) -> Result<Option<(String, &'a Value)>> {
    fn visit<'a>(
        view: &View,
        wanted: &str,
        value: &'a Value,
        pointer: &str,
    ) -> Result<(String, Option<(String, &'a Value)>)> {
        let digest = match value {
            Value::Array(values) => {
                let mut digests = Vec::with_capacity(values.len());
                for (index, child) in values.iter().enumerate() {
                    let (digest, found) = visit(
                        view,
                        wanted,
                        child,
                        &pointer_child(pointer, &index.to_string()),
                    )?;
                    if found.is_some() {
                        return Ok((String::new(), found));
                    }
                    digests.push(digest);
                }
                hash((TREE_SCHEMA, "array", digests))?
            }
            Value::Object(object) => {
                let mut digests = Vec::with_capacity(object.len());
                for (key, child) in object {
                    let (digest, found) = visit(view, wanted, child, &pointer_child(pointer, key))?;
                    if found.is_some() {
                        return Ok((String::new(), found));
                    }
                    digests.push((key, digest));
                }
                digests.sort_unstable_by(|left, right| left.0.cmp(right.0));
                hash((TREE_SCHEMA, "object", digests))?
            }
            _ => merkle(value)?,
        };
        let found = (hole_id(&view.basis, pointer, &digest)? == wanted)
            .then(|| (pointer.to_owned(), value));
        Ok((digest, found))
    }

    Ok(visit(view, wanted, value, pointer)?.1)
}

fn budget() -> Value {
    json!({
        "limit": 0,
        "used_upper_bound": 0,
        "unit": "conservative-model-token-upper-bound",
        "accounting": "serialized UTF-8 bytes including trailing newline",
        "compatibility": "byte-fallback tokenizers",
        "enforcement": "server"
    })
}

fn set_budget(report: &mut Value, limit: usize) -> Result<usize> {
    report["token_budget"] = budget();
    report["token_budget"]["limit"] = json!(limit);
    let mut prior = usize::MAX;
    loop {
        let used = serde_json::to_vec(report)?.len() + 1;
        if used == prior {
            return Ok(used);
        }
        report["token_budget"]["used_upper_bound"] = json!(used);
        prior = used;
    }
}

fn fits(report: &mut Value, options: &Options) -> Result<bool> {
    let used = set_budget(report, options.token_limit)?;
    Ok(disclosure_budget_admitted(
        used,
        options.token_limit,
        options.profile.code(),
    ))
}

fn base(project: &Project<'_>, view: &View, options: &Options) -> Result<Value> {
    let id = project.resolve_handle(&view.target)?;
    let node = &project.nodes[id];
    let mut report = project.envelope("disclose");
    report["schema"] = json!(SCHEMA);
    report["profile"] = json!(options.profile);
    report["view"] = json!(view.kind);
    report["target"] = json!({
        "handle": view.target,
        "name": node.name,
        "kind": node.kind,
        "path": node.path,
        "location": node.symbol.and_then(|symbol| project.index.symbol(symbol))
            .map(|symbol| project.definition_location(symbol))
    });
    report["view_basis"] = json!(view.basis);
    report["commitment"] = json!({
        "schema": TREE_SCHEMA,
        "root": view.root,
        "semantic_root": view.semantic_root,
        "tree_root": view.semantic_root,
        "object_root": view.object_root,
        "object_schema": OBJECT_SCHEMA,
        "source_root": view.source_root,
        "semantic_basis": view.semantic_basis,
        "algorithm": "sha256-tagged-canonical-json-tree",
        "object_algorithm": "sha256-tagged-binary-json-tree"
    });
    if options.proofs {
        report["commitment"]["proof_schema"] = json!(PROOF_SCHEMA);
    }
    project.response_context(None)?.apply(&mut report)?;
    Ok(report)
}

impl Project<'_> {
    pub(super) fn native_intent_packet(
        &self,
        intent: &super::agent_intent::Manifest,
        manifest_sha256: &str,
    ) -> Result<Value> {
        let options = Options {
            target: intent.target.clone(),
            reveal: None,
            cursor: None,
            token_limit: intent.token_limit,
            profile: AgentProfile::Compact,
            view: DisclosureView::Evidence,
            depth: 3,
            proofs: false,
        };
        let view = self.disclosure_view(&options)?;
        let id = self.resolve_handle(&view.target)?;
        let node = &self.nodes[id];
        let mut selected = serde_json::Map::new();
        let mut object_digests = serde_json::Map::new();
        for need in &intent.needs {
            let section = view
                .model
                .get(&need.section)
                .with_context(|| format!("agent intent section '{}' is absent.", need.section))?;
            let value = relative_pointer(section, &need.pointer).with_context(|| {
                format!(
                    "agent intent projection '{}' is absent from section '{}'.",
                    need.pointer, need.section
                )
            })?;
            selected.insert(need.name.clone(), value.clone());
            object_digests.insert(need.name.clone(), json!(object_merkle(value)?));
        }
        let purpose = serde_json::to_value(intent.purpose)?;
        let mut report = self.envelope("intent");
        report["schema"] = json!(super::agent_intent::RESULT_SCHEMA);
        report["intent"] = json!({
            "schema": super::agent_intent::SCHEMA,
            "purpose": purpose,
            "manifest_sha256": manifest_sha256,
            "basis": format!("frai1:{manifest_sha256}")
        });
        report["view_basis"] = json!(view.basis);
        report["object_root"] = json!(view.object_root);
        report["view"] = json!(view.kind);
        report["profile"] = json!(options.profile);
        report["target"] = json!({
            "handle": view.target,
            "name": node.name,
            "kind": node.kind,
            "path": node.path,
            "location": node.symbol.and_then(|symbol| self.index.symbol(symbol))
                .map(|symbol| self.definition_location(symbol))
        });
        report["calls"] = json!(0);
        report["selected"] = Value::Object(selected);
        report["object_digests"] = Value::Object(object_digests);
        report["cached_objects"] = json!([]);
        report["execution"] = json!({
            "engine": "native",
            "project_snapshots": 1,
            "progressive_disclosure_calls": 0
        });
        report["limits"] = json!({
            "packet_bytes": intent.packet_limit,
            "progressive_disclosure_calls": intent.call_limit
        });
        report["serialized_bytes"] = json!(0);
        Ok(report)
    }

    fn cross_stack_project_model(&self, selected: usize) -> Result<Value> {
        let mut scope = selected;
        while self.nodes[scope].symbol.is_some() {
            scope = self.nodes[scope]
                .parent
                .context("selected declaration has no containing file")?;
        }
        let target = self.handle(scope);
        let relationship = || super::RelationshipOptions {
            target: target.clone(),
            revision: None,
            limit: 500,
            cursor: None,
        };
        let technologies = self.technologies(&super::technologies::Options {
            selection: relationship(),
            evidence_limit: 4,
        })?;
        let applications = self.features(&super::FeatureOptions {
            selection: relationship(),
            feature: None,
        })?;
        let styles = self.styles(&relationship())?;
        let diagrams = self.diagrams(&relationship())?;
        let packages = self.packages(500, None)?;
        let dependencies = self.dependencies(None, 500, None)?;
        let resolutions = self.resolutions(None, None, 500, None)?;
        let mut feature_rows = Vec::new();
        let mut feature_gaps = Vec::new();
        let mut feature_total = 0usize;
        for manifest in self
            .manifests
            .documents
            .keys()
            .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
        {
            match super::package_features::evaluate(&self.manifests, manifest, &[], false) {
                Ok(evaluation) => {
                    feature_total += evaluation.rows.len();
                    feature_rows.extend(
                        evaluation
                            .rows
                            .into_iter()
                            .take(500usize.saturating_sub(feature_rows.len())),
                    );
                }
                Err(error) => feature_gaps.push(json!({
                    "manifest": bounded_text(&manifest.to_string_lossy(), 512),
                    "reason": bounded_text(&error.to_string(), 512),
                    "scope": "cargo-feature-activation"
                })),
            }
        }
        let manifest_gap_total = self.manifests.gaps.len();
        let lockfile_gap_total = self.lockfiles.gaps.len();
        Ok(json!({
            "schema": "fr-cross-stack-project-3",
            "scope": {"target": target, "selected": self.handle(selected)},
            "technologies": {
                "schema": technologies["technology_schema"],
                "items": technologies["items"], "page": technologies["page"],
                "analysis": technologies["analysis"]
            },
            "packages": {
                "manifests": {"items": packages["items"], "page": packages["page"]},
                "declarations": {"items": dependencies["items"], "page": dependencies["page"]},
                "resolutions": {"items": resolutions["items"], "page": resolutions["page"]},
                "feature_activations": {
                    "items": feature_rows,
                    "total": feature_total,
                    "omitted": feature_total.saturating_sub(500),
                    "gaps": feature_gaps.iter().take(64).collect::<Vec<_>>(),
                    "gaps_omitted": feature_gaps.len().saturating_sub(64),
                    "selection": {"default_features": true, "requested": []}
                },
                "gaps": {
                    "manifests": self.manifests.gaps.iter().take(500).collect::<Vec<_>>(),
                    "lockfiles": self.lockfiles.gaps.iter().take(500).collect::<Vec<_>>(),
                    "omitted": manifest_gap_total.saturating_sub(500) + lockfile_gap_total.saturating_sub(500)
                }
            },
            "applications": {
                "items": applications["items"], "page": applications["page"],
                "analysis": applications["analysis"]
            },
            "styles": {
                "schema": styles["style_schema"], "items": styles["items"],
                "page": styles["page"], "analysis": styles["analysis"]
            },
            "documents_and_diagrams": {
                "schema": diagrams["diagram_schema"], "items": diagrams["items"],
                "page": diagrams["page"], "analysis": diagrams["analysis"]
            }
        }))
    }

    fn disclosure_view(&self, options: &Options) -> Result<View> {
        ensure!(
            disclosure_view_admitted(options.view as usize, options.depth),
            "analysis disclosure depth must be between 0 and 8."
        );
        ensure!(
            options.target.starts_with("frp1:"),
            "project disclose requires a full revision-bound declaration handle."
        );
        let id = self.resolve_handle(&options.target)?;
        if options.view == DisclosureView::Semantic {
            ensure!(
                self.nodes[id].symbol.is_some(),
                "semantic disclosure requires a declaration handle."
            );
        }
        let semantic = if options.view == DisclosureView::Semantic {
            let semantic = self.semantic(&super::semantic::Options {
                provenance: Default::default(),
                target: options.target.clone(),
                revision: None,
                declaration: None,
                body: true,
                pointers: false,
                locators: false,
                locators_only: false,
                locator_op: None,
                locator_from: None,
                intent_to: None,
                nodes: 4096,
                unsupported_source: false,
                minimal: false,
            })?;
            ensure!(
                semantic["status"] == "returned",
                "complete semantic IR exceeds the disclosure analysis bound."
            );
            Some(semantic)
        } else {
            None
        };
        let (model, semantic_basis) = match semantic.as_ref() {
            Some(semantic) => (
                semantic["model"].clone(),
                semantic["semantic_basis"]
                    .as_str()
                    .context("semantic response omitted its basis")?
                    .to_owned(),
            ),
            None if options.view == DisclosureView::Evidence => {
                let model = self.evidence_model(id, options.depth)?;
                let basis = format!(
                    "frpe1:{}",
                    hash((
                        "fr-project-evidence-1",
                        &self.revision,
                        &options.target,
                        options.depth,
                        object_merkle(&model)?
                    ))?
                );
                (model, basis)
            }
            None if options.view == DisclosureView::Application => {
                let model =
                    serde_json::to_value(self.application_model(&options.target, None, None)?)?;
                let basis = format!(
                    "frpa1:{}",
                    hash((
                        crate::application_ir::SCHEMA,
                        &self.revision,
                        &options.target,
                        object_merkle(&model)?
                    ))?
                );
                (model, basis)
            }
            None => {
                let model = self.cross_stack_project_model(id)?;
                let basis = format!(
                    "frpx1:{}",
                    hash((
                        "fr-cross-stack-project-3",
                        &self.revision,
                        &options.target,
                        object_merkle(&model)?
                    ))?
                );
                (model, basis)
            }
        };
        let semantic_root = merkle(&model)?;
        let object_root = object_merkle(&model)?;
        let mut edits = Vec::new();
        let mut ir_edits = Vec::new();
        let node = &self.nodes[id];
        if semantic
            .as_ref()
            .is_some_and(|semantic| semantic["body_identity"]["status"] == "available")
            && node
                .symbol
                .and_then(|symbol| self.index.symbol(symbol))
                .is_some_and(|symbol| super::author::semantic_body_authorable(symbol.language))
        {
            if let Some(body) = model.pointer("/items/0/value/body") {
                let candidate = super::semantic_change::SemanticBody {
                    schema: super::semantic_ir::BODY_SCHEMA.to_owned(),
                    body: serde_json::from_value(body.clone())
                        .context("selected semantic body no longer matches the typed IR")?,
                };
                let value = serde_json::to_value(&candidate)?;
                if super::semantic_ir::source_free(&value)
                    && super::semantic_change::semantic_change_result_bounded(
                        candidate.body.len(),
                        super::semantic::semantic_nodes(&value),
                    )
                {
                    let body_basis = super::semantic_change::body_basis(&candidate)?;
                    for scalar in super::semantic_intent::body_scalar_targets(&candidate)? {
                        let pointer = format!("/model/items/0/value{}", scalar.scalar_pointer);
                        ensure!(
                            model.pointer(pointer.trim_start_matches("/model"))
                                == Some(&scalar.from),
                            "disclosed semantic scalar does not match its typed intent target."
                        );
                        edits.push(DisclosedEdit {
                            id: disclosed_edit_id(
                                &self.revision,
                                &options.target,
                                &body_basis,
                                &scalar,
                            )?,
                            pointer,
                            operation: scalar.operation,
                            from: scalar.from,
                        });
                    }
                    for structural in super::semantic_change::body_structural_targets(&candidate)? {
                        let pointer = format!("/model/items/0/value{}", structural.address_pointer);
                        ensure!(
                            model
                                .pointer(pointer.trim_start_matches("/model"))
                                .is_some(),
                            "disclosed structural edit does not match its semantic address."
                        );
                        ir_edits.push(DisclosedIrEdit {
                            id: disclosed_ir_edit_id(
                                &self.revision,
                                &options.target,
                                &body_basis,
                                &structural,
                            )?,
                            pointer,
                            target: structural,
                        });
                    }
                }
            }
        }
        let source = if matches!(
            options.view,
            DisclosureView::Project | DisclosureView::Application
        ) {
            String::new()
        } else {
            let (source, span) = self.source(id)?;
            source[span.start..span.end].to_owned()
        };
        let source_root = hash((TREE_SCHEMA, "source", source.as_bytes()))?;
        let root = hash((
            TREE_SCHEMA,
            "view",
            &self.revision,
            &options.target,
            &semantic_root,
            &source_root,
        ))?;
        let basis_digest = match options.view {
            DisclosureView::Semantic => hash((
                SCHEMA,
                &self.revision,
                &options.target,
                &semantic_basis,
                &root,
                options.profile,
                options.token_limit,
            ))?,
            DisclosureView::Evidence => hash((
                SCHEMA,
                &self.revision,
                &options.target,
                &semantic_basis,
                &root,
                options.profile,
                options.token_limit,
                options.view,
                options.depth,
            ))?,
            DisclosureView::Project | DisclosureView::Application => hash((
                SCHEMA,
                &self.revision,
                &options.target,
                &semantic_basis,
                &root,
                options.profile,
                options.token_limit,
                options.view,
            ))?,
        };
        let basis = format!("frdv1:{basis_digest}");
        Ok(View {
            target: options.target.clone(),
            semantic_basis,
            model,
            semantic_root,
            object_root,
            source_root,
            root,
            basis,
            source,
            kind: options.view,
            edits,
            ir_edits,
        })
    }

    pub(super) fn disclose(&self, options: &Options) -> Result<Value> {
        ensure!(
            disclosure_budget_admitted(0, options.token_limit, options.profile.code()),
            "token limit must be 1024..4096 for compact or 1024..16384 for expanded disclosure."
        );
        let view = self.disclosure_view(options)?;
        let mut report = base(self, &view, options)?;
        match options.reveal.as_deref() {
            None => {
                ensure!(options.cursor.is_none(), "--cursor requires --reveal.");
                report["status"] = json!("frontier");
                report["frontier"] = if matches!(
                    options.view,
                    DisclosureView::Project | DisclosureView::Application
                ) {
                    json!([semantic_hole(&view, options, "/model", &view.model)?])
                } else {
                    json!([
                        semantic_hole(&view, options, "/model", &view.model)?,
                        source_hole(&view, options, 0, None)?
                    ])
                };
                let mut shortcuts = match options.view {
                    DisclosureView::Semantic => semantic_shortcuts(&view, options)?,
                    DisclosureView::Evidence => evidence_shortcuts(&view, options)?,
                    DisclosureView::Project => project_shortcuts(&view, options)?,
                    DisclosureView::Application => project_shortcuts(&view, options)?,
                };
                let shortcut_field = match options.view {
                    DisclosureView::Semantic => "semantic_shortcuts",
                    DisclosureView::Evidence => "evidence_shortcuts",
                    DisclosureView::Project => "project_shortcuts",
                    DisclosureView::Application => "application_shortcuts",
                };
                report[shortcut_field] = json!(shortcuts);
                if options.view == DisclosureView::Evidence {
                    report["evidence_catalog"] = json!(evidence_catalog(&view)?);
                } else if options.view == DisclosureView::Project {
                    report["project_catalog"] = json!(project_catalog(&view)?);
                }
                report["instructions"] = match options.view {
                    DisclosureView::Semantic => json!("Prefer a relevant semantic_shortcuts action. Editable counts identify authorable scalar and IR descendants without revealing them. Structural IR descriptors require the expanded profile. Reveal the semantic root for complete hierarchy or the exact-source hole only when source is necessary."),
                    DisclosureView::Evidence => json!("Follow evidence_shortcuts; reveal source only when needed, and add --proofs only to verify a subtree."),
                    DisclosureView::Project => json!("Prefer a relevant project_shortcuts action for technologies, packages, applications, styles or documents_and_diagrams. Follow exact returned actions and reveal exact source only when a high-level fact or explicit gap is insufficient."),
                    DisclosureView::Application => json!("Follow application_shortcuts to application children or reader omissions. Fact identities, confidence and conversion boundaries are retained. Request only relevant descendants; object digests address reusable Merkle objects. The view supplies no source text or runtime proof."),
                };
                report["shortcut_budget"] = json!({
                    "limit": options.profile.row_limit(),
                    "returned": shortcuts.len(),
                    "additional_nodes": "reveal-semantic-root"
                });
                while !fits(&mut report, options)? && !shortcuts.is_empty() {
                    shortcuts.pop();
                    report[shortcut_field] = json!(shortcuts);
                    report["shortcut_budget"]["returned"] = json!(shortcuts.len());
                }
                ensure!(fits(&mut report, options)?, "the initial disclosure envelope does not fit this token limit; raise --token-limit or reuse --context-basis.");
                Ok(report)
            }
            Some(wanted) if wanted.starts_with("frs1:") => {
                self.reveal_source(options, &view, report, wanted)
            }
            Some(wanted) if wanted.starts_with("frh1:") => {
                self.reveal_semantic(options, &view, report, wanted)
            }
            Some(_) => bail!("invalid disclosure hole; use an exact returned reveal action."),
        }
    }

    fn reveal_source(
        &self,
        options: &Options,
        view: &View,
        base: Value,
        wanted: &str,
    ) -> Result<Value> {
        let offset = cursor_offset(view, options, wanted)?;
        ensure!(
            disclosure_transition_allowed(1, offset, view.source.len()),
            "source disclosure cursor is beyond the committed source."
        );
        ensure!(
            source_hole_id(&view.basis, &view.source_root, offset)? == wanted,
            "stale or conflicting source hole; use an exact returned reveal action."
        );
        let id = self.resolve_handle(&view.target)?;
        let (source, definition) = self.source(id)?;
        ensure!(
            source[definition.start..definition.end] == view.source,
            "committed source no longer matches its target definition."
        );
        let lines = LineIndex::new(source);
        let mut best = None;
        let mut low = 1usize;
        let mut high = view.source.len().saturating_sub(offset).max(1);
        while low <= high {
            let requested = low + (high - low) / 2;
            let Some(length) = source_slice_length(&view.source, offset, requested) else {
                bail!("source disclosure offset is not a UTF-8 boundary.");
            };
            if length == 0 && offset < view.source.len() {
                low = requested + 1;
                continue;
            }
            let end = offset + length;
            let location = lines.locate(
                Span::new(definition.start + offset, definition.start + end),
                source,
            );
            let mut report = base.clone();
            report["status"] = json!("revealed");
            report["revealed"] = json!({
                "id": wanted, "domain": "exact-source", "digest": view.source_root,
                "offset": offset, "returned_bytes": length, "total_bytes": view.source.len(),
                "text": &view.source[offset..end],
                "location": location
            });
            report["supersedes"] = json!(wanted);
            report["frontier"] = if end < view.source.len() {
                let next_cursor = cursor(
                    view,
                    options,
                    &source_hole_id(&view.basis, &view.source_root, end)?,
                    end,
                )?;
                json!([source_hole(view, options, end, Some(&next_cursor))?])
            } else {
                json!([])
            };
            let introduced = u64::from(end < view.source.len());
            ensure!(
                disclosure_frontier_after(1, introduced).is_some(),
                "invalid source frontier transition."
            );
            report["frontier_delta"] = json!({
                "removed": 1,
                "introduced": introduced,
                "complete": end == view.source.len()
            });
            if fits(&mut report, options)? {
                best = Some(report);
                if end == view.source.len() {
                    break;
                }
                low = requested + 1;
            } else {
                high = requested - 1;
            }
        }
        best.context("no exact source fragment fits this token limit; raise --token-limit or reuse --context-basis.")
    }

    fn reveal_semantic(
        &self,
        options: &Options,
        view: &View,
        base: Value,
        wanted: &str,
    ) -> Result<Value> {
        let (pointer, value) = find_hole(view, wanted, &view.model, "/model")?
            .context("stale or unknown semantic hole; use an exact returned reveal action.")?;
        let start = cursor_offset(view, options, wanted)?;
        match value {
            Value::Object(object) => self.reveal_children(
                options,
                view,
                base,
                wanted,
                ChildReveal {
                    pointer: &pointer,
                    node: value,
                    children: object
                        .iter()
                        .map(|(key, value)| (key.clone(), value))
                        .collect(),
                    start,
                },
            ),
            Value::Array(values) => self.reveal_children(
                options,
                view,
                base,
                wanted,
                ChildReveal {
                    pointer: &pointer,
                    node: value,
                    children: values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| (index.to_string(), value))
                        .collect(),
                    start,
                },
            ),
            Value::String(text) => self.reveal_string(
                options,
                view,
                base,
                wanted,
                StringReveal {
                    pointer: &pointer,
                    text,
                    start,
                },
            ),
            _ => {
                ensure!(start == 0, "semantic scalar cursor is beyond its value.");
                let mut report = base;
                report["status"] = json!("revealed");
                report["revealed"] = json!({"id": wanted, "domain": tree_domain(view), "address": format!("{}#{pointer}", view.semantic_basis), "digest": merkle(value)?, "object_digest": object_merkle(value)?, "value": value});
                attach_proof(&mut report, options, view, &pointer, value)?;
                report["supersedes"] = json!(wanted);
                report["frontier"] = json!([]);
                ensure!(
                    disclosure_frontier_after(1, 0) == Some(0),
                    "invalid scalar frontier transition."
                );
                report["frontier_delta"] = json!({"removed": 1, "introduced": 0, "complete": true});
                ensure!(
                    fits(&mut report, options)?,
                    "semantic scalar does not fit this token limit; raise --token-limit."
                );
                Ok(report)
            }
        }
    }

    fn reveal_children(
        &self,
        options: &Options,
        view: &View,
        base: Value,
        wanted: &str,
        reveal: ChildReveal<'_>,
    ) -> Result<Value> {
        let ChildReveal {
            pointer,
            node,
            children,
            start,
        } = reveal;
        ensure!(
            disclosure_transition_allowed(0, start, children.len()),
            "semantic disclosure cursor is beyond the child set."
        );
        let mut rows = Vec::new();
        let mut best = None;
        for (key, value) in children.iter().skip(start) {
            let child_pointer = pointer_child(pointer, key);
            let child_digest = merkle(value)?;
            let inline =
                !matches!(value, Value::Array(_) | Value::Object(_)) && scalar_is_inline(value)?;
            let mut row = if inline {
                json!({"key": key, "digest": child_digest, "object_digest": object_merkle(value)?, "value": value})
            } else {
                json!({"key": key, "hole": semantic_hole(view, options, &child_pointer, value)?})
            };
            if inline {
                if let Some(edit) = edit_descriptor(view, &child_pointer)? {
                    row["edit"] = edit;
                }
            }
            let child_ir_edits = ir_edit_descriptors(view, options.profile, &child_pointer)?;
            if !child_ir_edits.is_empty() {
                row["ir_edits"] = json!(child_ir_edits);
            }
            rows.push(row);
            let end = start + rows.len();
            let next = (end < children.len())
                .then(|| cursor(view, options, wanted, end))
                .transpose()?;
            let mut report = base.clone();
            report["status"] = json!("revealed");
            report["revealed"] = json!({
                "id": wanted, "domain": tree_domain(view), "address": format!("{}#{pointer}", view.semantic_basis),
                "digest": merkle(node)?,
                "object_digest": object_merkle(node)?,
                "value_kind": value_kind(node),
                "children": rows,
                "page": {"total": children.len(), "before": start, "returned": end - start, "remaining": children.len() - end, "next": next}
            });
            attach_proof(&mut report, options, view, pointer, node)?;
            let node_ir_edits = ir_edit_descriptors(view, options.profile, pointer)?;
            if !node_ir_edits.is_empty() {
                report["revealed"]["ir_edits"] = json!(node_ir_edits);
            }
            if let Some(next) = next.as_deref() {
                report["continuation"] = json!({
                    "reason": "more-children",
                    "arguments": arguments(options, wanted, Some(next))
                });
            }
            if end == children.len() {
                report["supersedes"] = json!(wanted);
            }
            let page_holes = rows.iter().filter(|row| row.get("hole").is_some()).count();
            let introduced = children
                .iter()
                .filter(|(_, value)| {
                    matches!(value, Value::Array(_) | Value::Object(_))
                        || serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() > 128)
                })
                .count();
            report["frontier_delta"] = json!({
                "removed": usize::from(end == children.len()),
                "introduced": if end == children.len() { introduced } else { 0 },
                "page_holes": page_holes,
                "complete": end == children.len()
            });
            ensure!(
                end != children.len() || disclosure_frontier_after(1, introduced as u64).is_some(),
                "invalid semantic frontier transition."
            );
            if fits(&mut report, options)? {
                best = Some(report);
            } else {
                break;
            }
        }
        if children.is_empty() {
            let mut report = base;
            report["status"] = json!("revealed");
            report["revealed"] = json!({"id": wanted, "domain": tree_domain(view), "address": format!("{}#{pointer}", view.semantic_basis), "digest": merkle(node)?, "object_digest": object_merkle(node)?, "children": [], "page": {"total": 0, "before": 0, "returned": 0, "remaining": 0, "next": null}});
            attach_proof(&mut report, options, view, pointer, node)?;
            report["supersedes"] = json!(wanted);
            let node_ir_edits = ir_edit_descriptors(view, options.profile, pointer)?;
            if !node_ir_edits.is_empty() {
                report["revealed"]["ir_edits"] = json!(node_ir_edits);
            }
            ensure!(
                disclosure_frontier_after(1, 0) == Some(0),
                "invalid empty-node frontier transition."
            );
            report["frontier_delta"] =
                json!({"removed": 1, "introduced": 0, "complete": true, "page_holes": 0});
            ensure!(
                fits(&mut report, options)?,
                "empty semantic node does not fit this token limit; raise --token-limit."
            );
            return Ok(report);
        }
        best.context("no semantic child fits this token limit; raise --token-limit or reuse --context-basis.")
    }

    fn reveal_string(
        &self,
        options: &Options,
        view: &View,
        base: Value,
        wanted: &str,
        reveal: StringReveal<'_>,
    ) -> Result<Value> {
        let StringReveal {
            pointer,
            text,
            start,
        } = reveal;
        ensure!(
            disclosure_transition_allowed(0, start, text.len()) && text.is_char_boundary(start),
            "semantic string cursor is beyond a UTF-8 boundary."
        );
        let mut best = None;
        let mut low = 1usize;
        let mut high = text.len().saturating_sub(start).max(1);
        while low <= high {
            let requested = low + (high - low) / 2;
            let Some(length) = source_slice_length(text, start, requested) else {
                bail!("semantic string offset is not a UTF-8 boundary.");
            };
            if length == 0 && start < text.len() {
                low = requested + 1;
                continue;
            }
            let end = start + length;
            let next = (end < text.len())
                .then(|| cursor(view, options, wanted, end))
                .transpose()?;
            let mut report = base.clone();
            report["status"] = json!("revealed");
            report["revealed"] = json!({"id": wanted, "domain": tree_domain(view), "address": format!("{}#{pointer}", view.semantic_basis), "digest": merkle(&json!(text))?, "object_digest": object_merkle(&json!(text))?, "value_fragment": &text[start..end], "offset": start, "returned_bytes": length, "total_bytes": text.len(), "next": next});
            attach_proof(&mut report, options, view, pointer, &json!(text))?;
            if let Some(next) = next.as_deref() {
                report["continuation"] = json!({
                    "reason": "more-string-bytes",
                    "arguments": arguments(options, wanted, Some(next))
                });
            }
            if end == text.len() {
                report["supersedes"] = json!(wanted);
                if let Some(edit) = edit_descriptor(view, pointer)? {
                    report["revealed"]["edit"] = edit;
                }
            }
            report["frontier_delta"] = json!({
                "removed": usize::from(end == text.len()),
                "introduced": 0,
                "complete": end == text.len()
            });
            if fits(&mut report, options)? {
                best = Some(report);
                if end == text.len() {
                    break;
                }
                low = requested + 1;
            } else {
                high = requested - 1;
            }
        }
        best.context("no semantic string fragment fits this token limit; raise --token-limit or reuse --context-basis.")
    }
}

fn relative_pointer<'a>(mut value: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(value);
    }
    for encoded in pointer.strip_prefix('/')?.split('/') {
        let part = encoded.replace("~1", "/").replace("~0", "~");
        value = match value {
            Value::Array(values) => {
                if part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || part.len() > 1 && part.starts_with('0')
                {
                    return None;
                }
                values.get(part.parse::<usize>().ok()?)?
            }
            Value::Object(object) => object.get(&part)?,
            _ => return None,
        };
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_objects_are_order_independent_and_arrays_are_order_sensitive() {
        assert_eq!(
            merkle(&json!({"a": 1, "b": 2})).unwrap(),
            merkle(&json!({"b": 2, "a": 1})).unwrap()
        );
        assert_ne!(
            merkle(&json!([1, 2])).unwrap(),
            merkle(&json!([2, 1])).unwrap()
        );
    }

    #[test]
    fn budget_and_frontier_kernels_cover_boundaries() {
        assert!(disclosure_budget_admitted(4096, 4096, 0));
        assert!(!disclosure_budget_admitted(4097, 4096, 0));
        assert!(disclosure_budget_admitted(16384, 16384, 1));
        assert_eq!(disclosure_frontier_after(2, 3), Some(4));
        assert_eq!(disclosure_frontier_after(0, 3), None);
    }
}
