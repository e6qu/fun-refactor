use super::hash;
#[cfg(test)]
use anyhow::bail;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

const SCHEMA: &str = "fr-context-1";
const OMITTED: [&str; 3] = ["coverage", "handle_prefix", "revision"];

pub(crate) struct ResponseContext {
    basis: String,
    revision: Value,
    handle_prefix: Value,
    coverage: Value,
    compact: bool,
}

impl ResponseContext {
    pub(crate) fn new(revision: &str, coverage: Value, supplied: Option<&str>) -> Result<Self> {
        let handle_prefix = format!("frp1:{}:", &revision[..32]);
        let basis = basis(revision, &handle_prefix, &coverage)?;
        if let Some(supplied) = supplied {
            ensure!(
                supplied == basis,
                "stale or conflicting context basis; request a full project report."
            );
        }
        Ok(Self {
            basis,
            revision: json!(revision),
            handle_prefix: json!(handle_prefix),
            coverage,
            compact: supplied.is_some(),
        })
    }

    pub(crate) fn apply(&self, report: &mut Value) -> Result<()> {
        let object = report
            .as_object_mut()
            .context("project response must be a JSON object")?;
        for (field, expected) in [
            ("revision", &self.revision),
            ("handle_prefix", &self.handle_prefix),
            ("coverage", &self.coverage),
        ] {
            ensure!(
                object.get(field) == Some(expected),
                "project response changed its {field} context after validation."
            );
        }
        object.insert("context_basis".into(), json!(self.basis));
        if self.compact {
            for field in OMITTED {
                object.remove(field);
            }
            object.insert("context_omitted".into(), json!(OMITTED));
        }
        Ok(())
    }
}

fn basis(revision: &str, handle_prefix: &str, coverage: &Value) -> Result<String> {
    Ok(format!(
        "frcb1:{}",
        hash((SCHEMA, revision, handle_prefix, coverage))?
    ))
}

#[cfg(test)]
fn reconstruct(compact: &Value, reviewed: &Value) -> Result<Value> {
    let compact = compact
        .as_object()
        .context("compact response must be a JSON object")?;
    let reviewed = reviewed
        .as_object()
        .context("reviewed response must be a JSON object")?;
    let compact_basis = compact
        .get("context_basis")
        .and_then(Value::as_str)
        .context("compact response has no context basis")?;
    ensure!(
        compact.get("context_omitted") == Some(&json!(OMITTED)),
        "compact response has a missing or unsupported omission declaration."
    );
    ensure!(
        reviewed.get("context_omitted").is_none(),
        "reviewed basis must be a full response"
    );
    let revision = reviewed
        .get("revision")
        .and_then(Value::as_str)
        .context("reviewed basis has no revision")?;
    let handle_prefix = reviewed
        .get("handle_prefix")
        .and_then(Value::as_str)
        .context("reviewed basis has no handle prefix")?;
    let coverage = reviewed
        .get("coverage")
        .context("reviewed basis has no coverage")?;
    let reviewed_basis = reviewed
        .get("context_basis")
        .and_then(Value::as_str)
        .context("reviewed response has no context basis")?;
    ensure!(
        compact_basis == reviewed_basis
            && reviewed_basis == basis(revision, handle_prefix, coverage)?,
        "stale or conflicting reviewed context basis."
    );
    let mut full = compact.clone();
    full.remove("context_omitted");
    for (field, value) in [
        ("revision", json!(revision)),
        ("handle_prefix", json!(handle_prefix)),
        ("coverage", coverage.clone()),
    ] {
        if full.insert(field.into(), value).is_some() {
            bail!("compact response conflicts with omitted field {field}.");
        }
    }
    Ok(Value::Object(full))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(query: &str, revision: &str, coverage: Value) -> Value {
        json!({
            "schema": "fr-project-1",
            "revision": revision,
            "handle_prefix": format!("frp1:{}:", &revision[..32]),
            "query": query,
            "coverage": coverage,
            "rows": []
        })
    }

    #[test]
    fn compact_response_reconstructs_exactly_from_another_reviewed_query() {
        let revision = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let coverage = json!({"indexed_files": 2, "files_by_gap": {}});
        let full_context = ResponseContext::new(revision, coverage.clone(), None).unwrap();
        let mut reviewed = report("map", revision, coverage.clone());
        full_context.apply(&mut reviewed).unwrap();
        let supplied = reviewed["context_basis"].as_str().unwrap();
        let compact_context =
            ResponseContext::new(revision, coverage.clone(), Some(supplied)).unwrap();
        let mut compact = report("find", revision, coverage.clone());
        compact_context.apply(&mut compact).unwrap();
        let mut expected = report("find", revision, coverage);
        full_context.apply(&mut expected).unwrap();
        assert_eq!(reconstruct(&compact, &reviewed).unwrap(), expected);
    }

    #[test]
    fn reconstruction_refuses_missing_truncated_stale_and_conflicting_bases() {
        let revision = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let coverage = json!({"indexed_files": 2});
        let context = ResponseContext::new(revision, coverage.clone(), None).unwrap();
        let mut reviewed = report("map", revision, coverage.clone());
        context.apply(&mut reviewed).unwrap();
        let supplied = reviewed["context_basis"].as_str().unwrap();
        let compact_context =
            ResponseContext::new(revision, coverage.clone(), Some(supplied)).unwrap();
        let mut compact = report("find", revision, coverage);
        compact_context.apply(&mut compact).unwrap();

        for broken in [
            {
                let mut value = reviewed.clone();
                value.as_object_mut().unwrap().remove("context_basis");
                value
            },
            {
                let mut value = reviewed.clone();
                value.as_object_mut().unwrap().remove("coverage");
                value
            },
            {
                let mut value = reviewed.clone();
                value["revision"] = json!("f".repeat(64));
                value
            },
        ] {
            assert!(reconstruct(&compact, &broken).is_err());
        }
        let mut conflicting = compact;
        conflicting["coverage"] = json!({"indexed_files": 99});
        assert!(reconstruct(&conflicting, &reviewed).is_err());
    }
}
