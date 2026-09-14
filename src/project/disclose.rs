use super::{bounded_text, hash, source_slice_length, AgentProfile, Project};
use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SCHEMA: &str = "fr-progressive-disclosure-1";
const TREE_SCHEMA: &str = "fr-semantic-merkle-1";
pub(crate) const EDIT_SCHEMA: &str = "fr-disclosed-edit-1";
pub(crate) const IR_EDIT_SCHEMA: &str = "fr-disclosed-ir-edit-1";
const INLINE_SCALAR_BYTES: usize = 128;
const MIN_TOKEN_LIMIT: usize = 1_024;

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
    #[arg(help = "Full revision-bound declaration handle.")]
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

#[derive(Clone)]
struct View {
    target: String,
    semantic_basis: String,
    model: Value,
    semantic_root: String,
    source_root: String,
    root: String,
    basis: String,
    source: String,
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
    if let Some(cursor) = cursor {
        args.extend(["--cursor".into(), cursor.into()]);
    }
    args
}

fn semantic_hole(view: &View, options: &Options, pointer: &str, value: &Value) -> Result<Value> {
    let digest = merkle(value)?;
    let id = hole_id(&view.basis, pointer, &digest)?;
    Ok(json!({
        "id": id,
        "domain": "semantic-ir",
        "address": format!("{}#{pointer}", view.semantic_basis),
        "digest": digest,
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
                    offset
                ))?
            ),
        "stale or conflicting disclosure cursor; use an exact returned reveal action."
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

fn base(project: &Project<'_>, view: &View, profile: AgentProfile) -> Result<Value> {
    let id = project.resolve_handle(&view.target)?;
    let node = &project.nodes[id];
    let mut report = project.envelope("disclose");
    report["schema"] = json!(SCHEMA);
    report["profile"] = json!(profile);
    report["target"] = json!({
        "handle": view.target,
        "name": node.name,
        "kind": node.kind,
        "path": node.path
    });
    report["view_basis"] = json!(view.basis);
    report["commitment"] = json!({
        "schema": TREE_SCHEMA,
        "root": view.root,
        "semantic_root": view.semantic_root,
        "source_root": view.source_root,
        "semantic_basis": view.semantic_basis,
        "algorithm": "sha256-tagged-canonical-json-tree"
    });
    project.response_context(None)?.apply(&mut report)?;
    Ok(report)
}

impl Project<'_> {
    fn disclosure_view(&self, options: &Options) -> Result<View> {
        ensure!(
            options.target.starts_with("frp1:"),
            "project disclose requires a full revision-bound declaration handle."
        );
        let id = self.resolve_handle(&options.target)?;
        ensure!(
            self.nodes[id].symbol.is_some(),
            "project disclose requires a declaration handle."
        );
        let semantic = self.semantic(&super::semantic::Options {
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
        let model = semantic["model"].clone();
        let semantic_basis = semantic["semantic_basis"]
            .as_str()
            .context("semantic response omitted its basis")?
            .to_owned();
        let semantic_root = merkle(&model)?;
        let mut edits = Vec::new();
        let mut ir_edits = Vec::new();
        let node = &self.nodes[id];
        if semantic["body_identity"]["status"] == "available"
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
        let (source, span) = self.source(id)?;
        let source = source[span.start..span.end].to_owned();
        let source_root = hash((TREE_SCHEMA, "source", source.as_bytes()))?;
        let root = hash((
            TREE_SCHEMA,
            "view",
            &self.revision,
            &options.target,
            &semantic_root,
            &source_root,
        ))?;
        let basis = format!(
            "frdv1:{}",
            hash((
                SCHEMA,
                &self.revision,
                &options.target,
                &semantic_basis,
                &root,
                options.profile,
                options.token_limit
            ))?
        );
        Ok(View {
            target: options.target.clone(),
            semantic_basis,
            model,
            semantic_root,
            source_root,
            root,
            basis,
            source,
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
        let mut report = base(self, &view, options.profile)?;
        match options.reveal.as_deref() {
            None => {
                ensure!(options.cursor.is_none(), "--cursor requires --reveal.");
                report["status"] = json!("frontier");
                report["frontier"] = json!([
                    semantic_hole(&view, options, "/model", &view.model)?,
                    source_hole(&view, options, 0, None)?
                ]);
                let mut shortcuts = semantic_shortcuts(&view, options)?;
                report["semantic_shortcuts"] = json!(shortcuts);
                report["instructions"] = json!("Prefer a relevant semantic_shortcuts action. Editable counts identify authorable scalar and IR descendants without revealing them. Structural IR descriptors require the expanded profile. Reveal the semantic root for complete hierarchy or the exact-source hole only when source is necessary.");
                report["shortcut_budget"] = json!({
                    "limit": options.profile.row_limit(),
                    "returned": shortcuts.len(),
                    "additional_nodes": "reveal-semantic-root"
                });
                while !fits(&mut report, options)? && !shortcuts.is_empty() {
                    shortcuts.pop();
                    report["semantic_shortcuts"] = json!(shortcuts);
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
            let mut report = base.clone();
            report["status"] = json!("revealed");
            report["revealed"] = json!({
                "id": wanted, "domain": "exact-source", "digest": view.source_root,
                "offset": offset, "returned_bytes": length, "total_bytes": view.source.len(),
                "text": &view.source[offset..end]
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
                report["revealed"] = json!({"id": wanted, "domain": "semantic-ir", "address": format!("{}#{pointer}", view.semantic_basis), "digest": merkle(value)?, "value": value});
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
                json!({"key": key, "digest": child_digest, "value": value})
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
                "id": wanted, "domain": "semantic-ir", "address": format!("{}#{pointer}", view.semantic_basis),
                "digest": merkle(node)?,
                "value_kind": value_kind(node),
                "children": rows,
                "page": {"total": children.len(), "before": start, "returned": end - start, "remaining": children.len() - end, "next": next}
            });
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
            report["revealed"] = json!({"id": wanted, "domain": "semantic-ir", "address": format!("{}#{pointer}", view.semantic_basis), "digest": merkle(node)?, "children": [], "page": {"total": 0, "before": 0, "returned": 0, "remaining": 0, "next": null}});
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
            report["revealed"] = json!({"id": wanted, "domain": "semantic-ir", "address": format!("{}#{pointer}", view.semantic_basis), "digest": merkle(&json!(text))?, "value_fragment": &text[start..end], "offset": start, "returned_bytes": length, "total_bytes": text.len(), "next": next});
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
