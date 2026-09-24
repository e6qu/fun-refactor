use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(super) struct Diagnostics {
    pub rows: Vec<Value>,
    pub cutoffs: BTreeSet<String>,
    pub build_success: Option<bool>,
    pub ignored_events: usize,
}

impl Diagnostics {
    pub fn read(stream: &Value, cargo: bool) -> Result<Self> {
        let text = stream["text"]
            .as_str()
            .context("compiler stream text is missing.")?;
        let retained = stream["retained_bytes"]
            .as_u64()
            .context("compiler stream size is missing.")?;
        let omitted = stream["omitted_bytes"]
            .as_u64()
            .context("compiler omission count is missing.")?;
        let mut parsed = Self {
            rows: Vec::new(),
            cutoffs: BTreeSet::new(),
            build_success: None,
            ignored_events: 0,
        };
        if omitted > 0 {
            parsed.cutoffs.insert("compiler-stream-truncated".into());
        }
        if text.contains('\u{fffd}') || retained != text.len() as u64 {
            parsed.cutoffs.insert("compiler-stream-encoding".into());
        }
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                parsed.cutoffs.insert("non-json-compiler-output".into());
                continue;
            };
            if cargo {
                match value["reason"].as_str() {
                    Some("compiler-message") => parsed.diagnostic(&value["message"], None, 0),
                    Some("build-finished") => {
                        if let Some(success) = value["success"].as_bool() {
                            if parsed.build_success.replace(success).is_some() {
                                parsed.cutoffs.insert("duplicate-build-outcome".into());
                            }
                        } else {
                            parsed.cutoffs.insert("invalid-build-outcome".into());
                        }
                    }
                    Some("compiler-artifact" | "build-script-executed") => {
                        parsed.ignored_events += 1
                    }
                    _ => {
                        parsed.cutoffs.insert("unknown-cargo-event".into());
                    }
                }
            } else if value["$message_type"] == "diagnostic" {
                parsed.diagnostic(&value, None, 0);
            } else if value["$message_type"] == "artifact" {
                parsed.ignored_events += 1;
            } else {
                parsed.cutoffs.insert("unknown-rustc-event".into());
            }
        }
        if cargo && parsed.build_success.is_none() {
            parsed.cutoffs.insert("missing-build-outcome".into());
        }
        Ok(parsed)
    }

    fn diagnostic(&mut self, value: &Value, parent: Option<usize>, depth: usize) {
        if depth > 8 || self.rows.len() >= 1024 {
            self.cutoffs.insert("diagnostic-budget".into());
            return;
        }
        let (Some(message), Some(level), Some(spans), Some(children)) = (
            value["message"].as_str(),
            value["level"].as_str(),
            value["spans"].as_array(),
            value["children"].as_array(),
        ) else {
            self.cutoffs.insert("malformed-diagnostic".into());
            return;
        };
        if !matches!(
            level,
            "error" | "warning" | "note" | "help" | "failure-note" | "ice"
        ) {
            self.cutoffs.insert("unknown-diagnostic-level".into());
        }
        if message.chars().count() > 512 || spans.len() > 16 {
            self.cutoffs.insert("diagnostic-detail-budget".into());
        }
        let index = self.rows.len();
        self.rows
            .push(json!({"index":index,"parent":parent,"level":level,
            "code":value["code"]["code"],"message":message.chars().take(512).collect::<String>(),
            "spans":spans.iter().take(16).cloned().collect::<Vec<_>>() }));
        for child in children {
            self.diagnostic(child, Some(index), depth + 1);
        }
    }
}
