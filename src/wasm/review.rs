//! Review refactorings on a private workspace before replacing the live state.

use super::{fail, ok, session_digest, Session, Workspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_REVIEW_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    Rename { path: String, line: usize, col: usize, name: String },
    RemoveFlag { flag: String, value: bool },
    ExtractVariable { path: String, range: String, name: String },
    ExtractFunction { path: String, range: String, name: String },
    InlineVariable { path: String, line: usize, col: usize },
    InlineCall { path: String, line: usize, col: usize },
    Signature { path: String, line: usize, col: usize, change: String },
    MoveSymbol { path: String, line: usize, col: usize, destination: String },
    OrganizeImports { path: String },
    Rewrite { path: String, line: usize, col: usize, which: String },
    Restructure { language: String, pattern: String, template: String },
    Delete { path: String, line: usize, col: usize },
    Translate { path: String, language: String },
}

impl Request {
    fn run(&self, workspace: &mut Workspace) -> String {
        match self {
            Self::Rename { path, line, col, name } => workspace.rename(path, *line, *col, name),
            Self::RemoveFlag { flag, value } => workspace.remove_flag(flag, *value),
            Self::ExtractVariable { path, range, name } => workspace.extract_variable(path, range, name),
            Self::ExtractFunction { path, range, name } => workspace.extract_function(path, range, name),
            Self::InlineVariable { path, line, col } => workspace.inline_variable(path, *line, *col),
            Self::InlineCall { path, line, col } => workspace.inline_call(path, *line, *col),
            Self::Signature { path, line, col, change } => workspace.signature(path, *line, *col, change),
            Self::MoveSymbol { path, line, col, destination } => workspace.move_symbol(path, *line, *col, destination),
            Self::OrganizeImports { path } => workspace.organize_imports(path),
            Self::Rewrite { path, line, col, which } => workspace.rewrite(path, *line, *col, which),
            Self::Restructure { language, pattern, template } => workspace.restructure(language, pattern, template),
            Self::Delete { path, line, col } => workspace.delete(path, *line, *col),
            Self::Translate { path, language } => workspace.translate(path, language),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Review {
    schema: String,
    workspace_basis: String,
    request: Request,
    result: Value,
    patch: Value,
    validation: String,
    native_checks: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    review: Review,
    basis: String,
}

fn digest(review: &Review) -> Result<String, String> {
    let encoded = serde_json::to_vec(review).map_err(|error| error.to_string())?;
    Ok(format!("frbr1:{}", hex::encode(Sha256::digest(encoded))))
}

fn answer(encoded: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(encoded).map_err(|error| error.to_string())?;
    if let Some(error) = value.get("error") {
        return Err(error.as_str().unwrap_or("browser operation failed").to_string());
    }
    Ok(value)
}

impl Workspace {
    fn prepare_review(&self, request: Request) -> Result<(Envelope, Workspace), String> {
        let result = (|| {
            let checkpoint = self.session();
            answer(&checkpoint)?;
            let session: Session = serde_json::from_str(&checkpoint).map_err(|error| error.to_string())?;
            let workspace_basis = session_digest(&session.body)?;
            let mut candidate = Workspace::restore_session(&checkpoint)?;
            let result = answer(&request.run(&mut candidate))?;
            // A successful edit must remain recoverable before it can replace the workspace.
            answer(&candidate.session())?;
            let patch = match result.get("transaction").and_then(Value::as_u64) {
                Some(transaction) => answer(&candidate.transaction_patch(transaction as u32, false))?,
                None => Value::Null,
            };
            let review = Review {
                schema: "fr-browser-review-1".to_string(),
                workspace_basis,
                request,
                result,
                patch,
                validation: "refactoring-rules-and-reparse".to_string(),
                native_checks: Vec::new(),
            };
            let basis = digest(&review)?;
            let envelope = Envelope { review, basis };
            if ok(&envelope).len() > MAX_REVIEW_BYTES {
                return Err("browser review exceeds its 16 MiB limit".to_string());
            }
            Ok((envelope, candidate))
        })();
        self.enter();
        result
    }
}

#[wasm_bindgen]
impl Workspace {
    /// Preview a JSON refactoring request without changing files or history.
    pub fn preview(&self, request: &str) -> String {
        if request.len() > MAX_REQUEST_BYTES {
            return fail("browser request exceeds its 64 KiB limit");
        }
        let request = match serde_json::from_str::<Request>(request) {
            Ok(request) => request,
            Err(error) => return fail(error),
        };
        match self.prepare_review(request) {
            Ok((review, _)) => ok(&review),
            Err(error) => fail(error),
        }
    }

    /// Recheck a saved review and replace the workspace only when it still matches.
    pub fn apply_review(&mut self, encoded: &str) -> String {
        if encoded.len() > MAX_REVIEW_BYTES {
            return fail("browser review exceeds its 16 MiB limit");
        }
        let envelope = match serde_json::from_str::<Envelope>(encoded) {
            Ok(envelope) => envelope,
            Err(error) => return fail(error),
        };
        if envelope.review.schema != "fr-browser-review-1"
            || digest(&envelope.review).as_ref() != Ok(&envelope.basis)
        {
            return fail("browser review schema or digest does not match");
        }
        // Source, retained history, request, warnings and output are all part of the review.
        match self.prepare_review(envelope.review.request) {
            Ok((fresh, candidate)) if fresh.basis == envelope.basis => {
                *self = candidate;
                self.enter();
                ok(&fresh.review.result)
            }
            Ok(_) => fail("browser review is stale; preview the change again"),
            Err(error) => fail(error),
        }
    }
}
