use super::{hash, occurrence::Occurrence, Project};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(super) fn analyzer_identity() -> Result<String> {
    hash([
        "python-scalar-fixed-point-2",
        env!("CARGO_PKG_VERSION"),
        include_str!("dataflow.rs"),
        include_str!("flow_summaries.rs"),
        include_str!("flow_modules.rs"),
        include_str!("control_flow.rs"),
        include_str!("flow_cache.rs"),
        include_str!("occurrence.rs"),
        include_str!("../span.rs"),
        include_str!("../parse.rs"),
        include_str!("../project.rs"),
        include_str!("manifests.rs"),
        include_str!("lockfiles.rs"),
        include_str!("../scan.rs"),
        include_str!("../index.rs"),
        include_str!("../extract.rs"),
        include_str!("../../Cargo.lock"),
    ])
}

fn origins(value: &mut Value, ids: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            for (old, new) in ids {
                if text.ends_with(old) {
                    text.replace_range(text.len() - old.len().., new);
                    break;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                origins(item, ids);
            }
        }
        Value::Object(fields) => {
            let previous = std::mem::take(fields);
            for (key, mut value) in previous {
                let mut key = json!(key);
                origins(&mut key, ids);
                origins(&mut value, ids);
                fields.insert(key.as_str().unwrap().into(), value);
            }
        }
        _ => (),
    }
}

impl Project<'_> {
    fn rebind_flow_occurrences(
        &self,
        value: &mut Value,
        files: &BTreeSet<PathBuf>,
        revision: &str,
        ids: &mut BTreeMap<String, String>,
    ) -> Result<()> {
        match value {
            Value::Object(fields)
                if fields
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.starts_with("fro1:")) =>
            {
                let old: Occurrence = serde_json::from_value(value.clone())?;
                ensure!(
                    old.revision == revision && files.contains(&self.root.join(&old.path)),
                    "cached occurrence escapes its input snapshot."
                );
                let expected = format!(
                    "fro1:{}",
                    hash((&old.revision, &old.path, old.location.span, &old.role))?
                );
                ensure!(
                    old.id == expected,
                    "cached occurrence identity differs from its source."
                );
                let fresh =
                    self.occurrence(&self.root.join(&old.path), old.location.span, &old.role)?;
                ensure!(
                    old.location == fresh.location,
                    "cached occurrence coordinates differ from their source."
                );
                ids.insert(old.id, fresh.id.clone());
                *value = json!(fresh);
            }
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    self.rebind_flow_occurrences(child, files, revision, ids)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    self.rebind_flow_occurrences(child, files, revision, ids)?;
                }
            }
            _ => (),
        }
        Ok(())
    }

    pub(super) fn reuse_flow(
        &self,
        path: &Path,
        digest: &str,
        inputs: &Value,
        file: &Path,
        target: &str,
    ) -> Result<Option<Value>> {
        ensure!(
            std::fs::metadata(path)?.len() <= 1_048_576,
            "retained flow exceeds 1 MiB"
        );
        let mut report: Value = serde_json::from_slice(&crate::vfs::read(path)?)?;
        ensure!(hash(&report)? == digest, "retained flow digest mismatch");
        ensure!(
            report["schema"] == "fr-dataflow-1",
            "expected a retained dataflow report"
        );
        if report["inputs"] != *inputs || report["complete"] != true {
            return Ok(None);
        }
        ensure!(
            report["input_digest"] == hash(inputs)?
                && report["semantics"]
                    == if inputs["summary_mode"] == true {
                        "python-scalar-summaries-1"
                    } else {
                        "python-scalar-fixed-point-2"
                    },
            "retained flow has inconsistent analysis identities."
        );
        let revision = report["revision"].as_str().unwrap_or_default().to_owned();
        let mut ids = BTreeMap::new();
        let files = if let Some(files) = inputs["modules"]["files"].as_object() {
            files.keys().map(|path| self.root.join(path)).collect()
        } else {
            BTreeSet::from([file.to_owned()])
        };
        self.rebind_flow_occurrences(&mut report, &files, &revision, &mut ids)?;
        origins(&mut report["function_summaries"], &ids);
        for field in ["returns", "exceptional_returns", "origins"] {
            if let Some(values) = report[field].as_object_mut() {
                let old = std::mem::take(values);
                for (key, mut trace) in old {
                    let mut key = json!(key);
                    origins(&mut key, &ids);
                    if let Some(origin) = trace.get_mut("origin") {
                        origins(origin, &ids);
                    }
                    values.insert(key.as_str().unwrap().into(), trace);
                }
            }
        }
        if let Some(witnesses) = report["witnesses"].as_array_mut() {
            for witness in witnesses.iter_mut() {
                origins(&mut witness["trace"]["origin"], &ids);
            }
            witnesses.sort_by(|a, b| {
                (
                    a["site"]["path"].as_str(),
                    a["site"]["location"]["span"]["start"].as_u64(),
                    a["trace"]["origin"].as_str(),
                )
                    .cmp(&(
                        b["site"]["path"].as_str(),
                        b["site"]["location"]["span"]["start"].as_u64(),
                        b["trace"]["origin"].as_str(),
                    ))
            });
        }
        if let Some(summaries) = report["summaries"].as_array_mut() {
            for summary in summaries {
                origins(&mut summary["inputs"], &ids);
                origins(&mut summary["return_origins"], &ids);
                if let Some(arguments) = summary["inputs"].as_array_mut() {
                    for argument in arguments {
                        if let Some(origins) = argument.as_array_mut() {
                            origins.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                        }
                    }
                }
                if let Some(origins) = summary["return_origins"].as_array_mut() {
                    origins.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                }
            }
        }
        report["revision"] = json!(self.revision);
        report["handle_prefix"] = json!(format!("frp1:{}:", &self.revision[..32]));
        report["coverage"] = self.coverage();
        report["target"] = json!(target);
        report.as_object_mut().unwrap().remove("context_basis");
        report["execution"] = json!({"kind":"retained", "analysis_steps":0,
            "trust":"caller-supplied digest binds retained analysis; no execution attestation or mutation authority"});
        Ok(Some(report))
    }
}
