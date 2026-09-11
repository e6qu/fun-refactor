use super::*;
use anyhow::{ensure, Context};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};

const SCHEMA: &str = "fr-project-batch-1";
const MAX_INPUT_BYTES: u64 = 65_536;

#[derive(clap::Args)]
pub struct Options {
    #[arg(
        long,
        help = "JSON batch manifest path, or - for standard input; at most 64 KiB."
    )]
    from: PathBuf,
    #[arg(
        long,
        default_value_t = 65_536,
        help = "Shared serialized report budget, from 256 through 1048576 bytes."
    )]
    report_bytes: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    requests: Vec<Request>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    arguments: Vec<String>,
}

#[derive(Parser)]
#[command(no_binary_name = false)]
struct NestedQuery {
    #[command(subcommand)]
    command: Command,
}

struct ParsedRequest {
    request: Request,
    command: Command,
}

fn read_manifest(root: &Path, path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("reading project batch manifest from standard input")?;
    } else {
        let path = root.join(path);
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "project batch manifest must be a regular file."
        );
        let file = fs::File::open(&path)
            .with_context(|| format!("opening project batch manifest {}", path.display()))?;
        ensure!(
            file.metadata()?.is_file(),
            "project batch manifest must be a regular file."
        );
        file.take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("reading project batch manifest")?;
    }
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "project batch manifest exceeds 64 KiB."
    );
    String::from_utf8(bytes).context("project batch manifest must use UTF-8.")
}

fn validate(manifest: Manifest) -> Result<Vec<ParsedRequest>> {
    ensure!(
        manifest.schema == SCHEMA,
        "project batch manifest schema must be {SCHEMA}."
    );
    ensure!(
        (1..=16).contains(&manifest.requests.len()),
        "project batch needs 1 through 16 requests."
    );
    let mut ids = BTreeSet::new();
    let mut argument_bytes = 0usize;
    let mut parsed = Vec::with_capacity(manifest.requests.len());
    for request in manifest.requests {
        ensure!(
            !request.id.is_empty()
                && request.id.len() <= 80
                && request
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
            "project batch request IDs need 1 through 80 ASCII letters, digits, '.', '_' or '-'."
        );
        ensure!(
            ids.insert(request.id.clone()),
            "project batch request IDs must be unique."
        );
        ensure!(
            (1..=64).contains(&request.arguments.len()),
            "each project batch request needs 1 through 64 arguments."
        );
        let request_bytes = request.arguments.iter().map(String::len).sum::<usize>();
        ensure!(
            request_bytes <= 4096,
            "each project batch request is limited to 4096 argument bytes."
        );
        argument_bytes = argument_bytes
            .checked_add(request_bytes)
            .context("project batch argument size overflow")?;
        let query = NestedQuery::try_parse_from(
            std::iter::once("fr-project").chain(request.arguments.iter().map(String::as_str)),
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            !matches!(query.command, Command::Batch(_)),
            "project batches cannot contain another batch."
        );
        parsed.push(ParsedRequest {
            request,
            command: query.command,
        });
    }
    ensure!(
        argument_bytes <= 16_384,
        "project batch requests exceed the combined 16384-byte argument limit."
    );
    Ok(parsed)
}

impl Project<'_> {
    pub(super) fn batch(&self, options: &Options) -> Result<Value> {
        ensure!(
            (256..=1_048_576).contains(&options.report_bytes),
            "report bytes must be between 256 and 1048576."
        );
        let source = read_manifest(&self.root, &options.from)?;
        let manifest: Manifest = serde_json::from_str(&source)
            .context("project batch input must be a batch manifest.")?;
        let manifest_basis = format!("frpqb1:{}", hash((SCHEMA, &manifest))?);
        let requests = validate(manifest)?;

        let common = self.envelope("batch");
        let mut used = 0usize;
        let mut omitted = 0usize;
        let mut results = Vec::with_capacity(requests.len());
        for parsed in requests {
            let request_basis = format!(
                "frpqr1:{}",
                hash((SCHEMA, &parsed.request.id, &parsed.request.arguments))?
            );
            let mut report = self
                .report(&parsed.command)
                .with_context(|| format!("project batch request '{}' failed", parsed.request.id))?;
            let object = report
                .as_object_mut()
                .context("project query response must be a JSON object")?;
            for field in ["schema", "revision", "handle_prefix", "coverage"] {
                ensure!(
                    object.get(field) == common.get(field),
                    "project batch request '{}' changed common field {field}.",
                    parsed.request.id
                );
                object.remove(field);
            }
            let bytes = serde_json::to_vec(&report)?.len();
            if batch_section_fits(used, bytes, options.report_bytes) {
                used += bytes;
                results.push(json!({
                    "id": parsed.request.id,
                    "query": report["query"],
                    "request_basis": request_basis,
                    "status": "returned",
                    "report_bytes": bytes,
                    "report": report
                }));
            } else {
                omitted += 1;
                results.push(json!({
                    "id": parsed.request.id,
                    "query": report["query"],
                    "request_basis": request_basis,
                    "status": "omitted-report-budget",
                    "required_report_bytes": bytes
                }));
            }
        }

        let mut result = common;
        result["manifest_basis"] = json!(manifest_basis);
        result["requests"] = json!(results);
        result["report_budget"] = json!({
            "limit_bytes": options.report_bytes,
            "returned_bytes": used,
            "omitted_requests": omitted,
            "scope": "serialized request reports; common context and request metadata excluded"
        });
        Ok(result)
    }
}
