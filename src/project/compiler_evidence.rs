use super::{compiler_diagnostics::Diagnostics, hash, page, Project};
use anyhow::{ensure, Context, Result};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    RustcJson,
    CargoJson,
}

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    from: PathBuf,
    #[arg(long)]
    digest: String,
    #[arg(long)]
    check: String,
    #[arg(long, value_enum, default_value = "rustc-json")]
    format: Format,
    #[arg(long, default_value_t = 8)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long, default_value_t = 32768)]
    bytes: usize,
}

pub fn compiler_evidence_complete(
    execution: bool,
    protocol: bool,
    from_start: bool,
    no_remaining: bool,
) -> bool {
    execution && protocol && from_start && no_remaining
}

impl Project<'_> {
    pub(super) fn compiler_evidence(&self, options: &Options) -> Result<Value> {
        ensure!(
            (1..=64).contains(&options.limit),
            "diagnostic limit must be between 1 and 64."
        );
        ensure!(
            (4096..=1_048_576).contains(&options.bytes),
            "compiler response budget must be between 4096 and 1048576."
        );
        ensure!(
            std::fs::metadata(&options.from)?.len() <= 1_048_576,
            "retained check report exceeds 1 MiB."
        );
        let bytes = crate::vfs::read(&options.from)?;
        ensure!(
            bytes.len() <= 1_048_576,
            "retained check report exceeds 1 MiB."
        );
        let report: Value = serde_json::from_slice(&bytes)?;
        ensure!(
            hash(&report)? == options.digest,
            "retained check report digest mismatch."
        );
        crate::checks::evidence::validate(&self.root, &report)?;
        let check = report["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == options.check)
            .context("selected check has no retained result.")?;
        let cargo = matches!(options.format, Format::CargoJson);
        let format = if cargo { "cargo-json" } else { "rustc-json" };
        let stream = if cargo { "stdout" } else { "stderr" };
        let mut parsed = Diagnostics::read(&check[stream], cargo)?;
        let execution = check["exit_code"].is_i64()
            && check["timed_out"] == false
            && check["output_limit_exceeded"] == false
            && check["error"].is_null()
            && check["termination_error"].is_null();
        if !execution {
            parsed.cutoffs.insert("unfinished-check".into());
        }
        let errors = parsed
            .rows
            .iter()
            .filter(|row| matches!(row["level"].as_str(), Some("error" | "ice")))
            .count();
        if errors > 0 && check["passed"] == true {
            parsed.cutoffs.insert("errors-with-passing-check".into());
        }
        if parsed
            .build_success
            .is_some_and(|success| check["passed"] != success)
        {
            parsed
                .cutoffs
                .insert("cargo-check-outcome-disagreement".into());
        }
        let analyzer = hash((
            "rust-compiler-evidence-1",
            include_str!("compiler_evidence.rs"),
            include_str!("compiler_diagnostics.rs"),
        ))?;
        let inputs = json!({"revision":self.revision,"source_revision":report["source_revision"],
            "configuration":report["basis"],"toolchain":hash(&report["toolchain"])?,
            "report":options.digest,"check":options.check,"format":format,"analyzer":analyzer});
        let basis = format!("frcpe1:{}", hash(&inputs)?);
        let cwd = check["cwd"]
            .as_str()
            .context("check directory is missing.")?;
        let mut rows = Vec::new();
        let mut disagreements = Vec::new();
        for mut core in parsed.rows {
            let mut spans = Vec::new();
            for span in core["spans"].as_array().unwrap() {
                spans.push(self.compiler_span(cwd, span)?);
            }
            core["spans"] = json!(spans);
            core["rule"] = json!("rust-compiler-evidence-1:diagnostic");
            core["confidence"] = json!("reported-by-declared-check");
            let id = format!("frcd1:{}", hash((&basis, &core))?);
            if matches!(core["level"].as_str(), Some("error" | "ice"))
                && spans.iter().any(|span| span["syntax"] == "accepted")
            {
                disagreements.push(json!({"diagnostic":id,"kind":"syntax-accepted-compiler-error",
                    "claim":"parser acceptance and compiler rejection concern different verification levels."}));
            }
            rows.push(json!({"id":id,"basis":basis,"core":core}));
        }
        let key = format!("frce-page:{}", hash(&basis)?);
        let (start, end, _) = page(rows.len(), options.limit, options.cursor.as_deref(), &key)?;
        let capture_complete = parsed.cutoffs.is_empty();
        let mut value = json!({"schema":"fr-compiler-evidence-1","revision":self.revision,
            "handle_prefix":format!("frp1:{}:",&self.revision[..32]),"coverage":self.coverage(),
            "basis":basis,"inputs":inputs,"analyzer":analyzer,"mutation_authority":false,
            "budget":{"limit":options.limit,"response_bytes":options.bytes},
            "scope":{"check":options.check,"format":format,"stream":stream,"argv":check["argv"],"cwd":cwd,"covers":check["covers"],
                "toolchain":report["toolchain"],"claim":"declared invocation and retained diagnostic stream; undeclared dependencies remain outside coverage."},
            "outcome":{"passed":check["passed"],"exit_code":check["exit_code"],"execution_complete":execution,"cargo_success":parsed.build_success},
            "capture":{"complete":capture_complete,"cutoffs":parsed.cutoffs,"diagnostics":rows.len(),"errors":errors,
                "ignored_protocol_events":parsed.ignored_events,"retained_bytes":check[stream]["retained_bytes"],"omitted_bytes":check[stream]["omitted_bytes"]},
            "items":rows[start..end],"disagreements":[],"continuation":Value::Null,
            "trust":"trusted caller digest binds retained check output; no independent execution attestation.",
            "claim":"compiler observations and exact syntax locations; no runtime safety or source implementation proof."});
        loop {
            let returned = value["items"].as_array().unwrap().len();
            let end = start + returned;
            let next = (end < rows.len()).then(|| format!("{key}:{end}"));
            value["page"] = json!({"before":start,"returned":returned,"remaining":rows.len()-end,"total":rows.len(),"next":next});
            let ids = value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["id"].clone())
                .collect::<Vec<_>>();
            value["disagreements"] = json!(disagreements
                .iter()
                .filter(|d| ids.contains(&d["diagnostic"]))
                .collect::<Vec<_>>());
            value["disclosure_complete"] = json!(start == 0 && end == rows.len());
            value["complete"] = json!(compiler_evidence_complete(
                execution,
                capture_complete,
                start == 0,
                end == rows.len()
            ));
            value["continuation"] = next.as_ref().map(|cursor| json!({"arguments":["project","compiler-evidence",
                "--from",options.from,"--digest",options.digest,"--check",options.check,"--format",format,
                "--limit",options.limit.to_string(),"--bytes",options.bytes.to_string(),"--cursor",cursor]})).unwrap_or(Value::Null);
            if serde_json::to_vec(&value)?.len() + 512 <= options.bytes {
                break;
            }
            ensure!(
                returned > 1,
                "compiler metadata or one diagnostic exceeds response budget; increase --bytes."
            );
            value["items"].as_array_mut().unwrap().pop();
        }
        Ok(value)
    }

    fn compiler_span(&self, cwd: &str, span: &Value) -> Result<Value> {
        let mut result = json!({"status":"unavailable","reason":"No admitted source location.","occurrence":Value::Null,
            "primary":span["is_primary"].as_bool(),"syntax":"unknown","source":Value::Null});
        if !span["expansion"].is_null() {
            result["status"] = json!("expanded");
            result["reason"] = json!("Macro expansion needs a separate origin relation.");
            return Ok(result);
        }
        let (Some(name), Some(start), Some(end), Some(primary)) = (
            span["file_name"].as_str(),
            span["byte_start"].as_u64(),
            span["byte_end"].as_u64(),
            span["is_primary"].as_bool(),
        ) else {
            return Ok(result);
        };
        let path = Path::new(name);
        let file = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(cwd).join(path)
        };
        let Ok(relative) = file.strip_prefix(&self.root) else {
            result["reason"] = json!("Compiler location is outside the selected workspace.");
            return Ok(result);
        };
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            result["reason"] = json!("Compiler location requires unsupported path normalization.");
            return Ok(result);
        }
        let file = self.root.join(
            relative
                .components()
                .filter_map(|c| {
                    if let Component::Normal(part) = c {
                        Some(part)
                    } else {
                        None
                    }
                })
                .collect::<PathBuf>(),
        );
        let Some(source) = self.sources.get(&file) else {
            result["reason"] = json!("Compiler location has no indexed source snapshot.");
            return Ok(result);
        };
        if self
            .index
            .file(&file)
            .is_none_or(|info| info.language != crate::lang::Language::Rust)
        {
            result["reason"] = json!("Compiler location is outside the Rust source subset.");
            return Ok(result);
        }
        if start > end
            || end > source.len() as u64
            || !source.is_char_boundary(start as usize)
            || !source.is_char_boundary(end as usize)
        {
            result["status"] = json!("invalid");
            result["reason"] = json!("Compiler byte span is outside source boundaries.");
            return Ok(result);
        }
        let occurrence = self.occurrence(
            &file,
            crate::span::Span::new(start as usize, end as usize),
            if primary {
                "compiler-primary"
            } else {
                "compiler-secondary"
            },
        )?;
        let mut parses = self.parses.borrow_mut();
        if !parses.contains_key(&file) {
            parses.insert(
                file.clone(),
                crate::parse::Parsers::new().parse(crate::lang::Language::Rust, source)?,
            );
        }
        let parsed = &parses[&file];
        result["status"] = json!("exact");
        result["reason"] = json!("Compiler byte offsets in the validated source snapshot.");
        result["syntax"] = json!(if parsed.has_errors() {
            "rejected"
        } else {
            "accepted"
        });
        result["occurrence"] = json!(occurrence);
        if let Some(node) = self
            .nodes
            .iter()
            .position(|node| node.path == occurrence.path && node.symbol.is_none())
        {
            result["source"] = json!({"arguments":["project","show",self.handle(node),"--source","--bytes","256","--offset",start.to_string()]});
        }
        Ok(result)
    }
}
