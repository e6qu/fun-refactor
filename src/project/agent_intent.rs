use super::{disclose, Project};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "fr-agent-intent-1";
pub const RESULT_SCHEMA: &str = "fr-agent-context-1";
const MAX_INPUT_BYTES: u64 = 65_536;

#[derive(Args)]
pub struct Options {
    #[arg(
        long,
        help = "Declarative fr-agent-intent-1 path, or - for standard input; at most 64 KiB."
    )]
    from: PathBuf,
    #[arg(long, help = "Execute one unchanged reviewed change intent.")]
    pub(crate) write: bool,
    #[arg(
        long,
        requires = "write",
        help = "Complete intent-action basis returned by preview."
    )]
    pub(crate) basis: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Purpose {
    Understand,
    Trace,
    Change,
    Migrate,
    Prove,
}

impl Purpose {
    pub(super) fn code(self) -> usize {
        match self {
            Self::Understand => 0,
            Self::Trace => 1,
            Self::Change => 2,
            Self::Migrate => 3,
            Self::Prove => 4,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Need {
    pub(super) name: String,
    pub(super) section: String,
    #[serde(default)]
    pub(super) pointer: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    schema: String,
    pub(super) target: String,
    pub(super) purpose: Purpose,
    pub(super) needs: Vec<Need>,
    pub(super) token_limit: usize,
    pub(super) call_limit: usize,
    pub(super) packet_limit: usize,
    #[serde(default)]
    action: Option<Action>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Action {
    task_change: Value,
    #[serde(default = "default_diff_bytes")]
    diff_bytes: usize,
    #[serde(default = "default_report_bytes")]
    report_bytes: usize,
}

pub(crate) struct Compiled {
    pub(crate) report: Value,
    pub(crate) action: Option<super::task_change::Prepared>,
    pub(crate) action_diff_bytes: usize,
    pub(crate) purpose: usize,
    needs: usize,
    sections: usize,
    call_limit: usize,
    packet_limit: usize,
}

fn default_diff_bytes() -> usize {
    4_096
}

fn default_report_bytes() -> usize {
    65_536
}

fn read_input(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("reading agent intent from standard input")?;
    } else {
        let path = root.join(path);
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "agent intent must be a regular file."
        );
        fs::File::open(&path)
            .with_context(|| format!("opening agent intent {}", path.display()))?
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)?;
    }
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "agent intent exceeds 64 KiB."
    );
    Ok(bytes)
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && name.len() <= 64
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn section_code(section: &str) -> Option<usize> {
    match section {
        "code_map" => Some(0),
        "call_traces" => Some(1),
        "impact" => Some(2),
        "sources_and_sinks" => Some(3),
        _ => None,
    }
}

fn pointer_well_formed(pointer: &str) -> bool {
    if pointer.is_empty() {
        return true;
    }
    if !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            index += 1;
            if index == bytes.len() || !matches!(bytes[index], b'0' | b'1') {
                return false;
            }
        }
        index += 1;
    }
    true
}

impl Manifest {
    fn validate(&self) -> Result<usize> {
        ensure!(
            self.schema == SCHEMA,
            "agent intent schema must be {SCHEMA}."
        );
        ensure!(
            self.target.starts_with("frp1:") && self.target.len() <= 16_384,
            "agent intent target must be a bounded full project handle."
        );
        ensure!(
            (1..=32).contains(&self.needs.len()),
            "agent intent needs 1 through 32 projections."
        );
        ensure!(
            (1_024..=4_096).contains(&self.token_limit),
            "agent intent token limit must be 1024 through 4096."
        );
        ensure!(
            (1..=512).contains(&self.call_limit),
            "agent intent call limit must be 1 through 512."
        );
        ensure!(
            (1_024..=65_536).contains(&self.packet_limit),
            "agent intent packet limit must be 1024 through 65536 bytes."
        );
        let mut names = BTreeSet::new();
        let mut sections = BTreeSet::new();
        for need in &self.needs {
            ensure!(valid_name(&need.name), "agent intent need name is invalid.");
            ensure!(
                names.insert(&need.name),
                "agent intent projection names must be unique."
            );
            let section =
                section_code(&need.section).context("agent intent need section is unsupported.")?;
            ensure!(
                agent_intent_section_allowed(self.purpose.code(), section),
                "agent intent need section is outside its declared purpose."
            );
            ensure!(
                pointer_well_formed(&need.pointer),
                "agent intent need pointer must be a canonical relative JSON Pointer."
            );
            sections.insert(section);
        }
        ensure!(
            sections.len() <= 8,
            "agent intent reaches at most eight evidence sections."
        );
        if let Some(action) = &self.action {
            ensure!(
                matches!(self.purpose, Purpose::Change),
                "agent intent actions require purpose 'change'."
            );
            let requests = action.task_change["requests"].as_array();
            let targets = action.task_change["targets"].as_array();
            ensure!(
                requests.is_some_and(Vec::is_empty)
                    && targets.is_some_and(|targets| targets.len() == 1),
                "agent intent action requires one direct task-change target and no project requests."
            );
            ensure!(
                targets.and_then(|targets| targets[0]["handle"].as_str())
                    == Some(self.target.as_str()),
                "agent intent action target must equal the intent target."
            );
            ensure!(
                action.diff_bytes <= 65_536,
                "agent intent action diff limit must be at most 65536 bytes."
            );
            ensure!(
                (256..=1_048_576).contains(&action.report_bytes),
                "agent intent action report limit must be 256 through 1048576 bytes."
            );
        }
        Ok(sections.len())
    }
}

pub fn agent_intent_section_allowed(purpose: usize, section_code: usize) -> bool {
    matches!(
        (purpose, section_code),
        (0, 0) | (1, 0 | 1 | 3) | (2, 0 | 2) | (3, 0 | 2 | 3) | (4, 0 | 2)
    )
}

impl Project<'_> {
    pub(crate) fn compile_agent_intent(&self, options: &Options) -> Result<Compiled> {
        let input = read_input(&self.root, &options.from)?;
        let manifest: Manifest =
            serde_json::from_slice(&input).context("agent intent must match fr-agent-intent-1.")?;
        let sections = manifest.validate()?;
        let manifest_sha256 = hex::encode(Sha256::digest(&input));
        let report = self.native_intent_packet(&manifest, &manifest_sha256)?;
        let action = manifest
            .action
            .as_ref()
            .map(|action| {
                let bytes = serde_json::to_vec(&action.task_change)?;
                self.task_change_bytes(&bytes, action.diff_bytes, action.report_bytes)
            })
            .transpose()?;
        Ok(Compiled {
            report,
            action,
            action_diff_bytes: manifest
                .action
                .as_ref()
                .map_or(0, |action| action.diff_bytes),
            purpose: manifest.purpose.code(),
            needs: manifest.needs.len(),
            sections,
            call_limit: manifest.call_limit,
            packet_limit: manifest.packet_limit,
        })
    }
}

pub fn agent_action_mode(
    purpose: usize,
    action_complete: bool,
    write: bool,
    basis_supplied: bool,
    basis_matches: bool,
) -> usize {
    match (
        purpose,
        action_complete,
        write,
        basis_supplied,
        basis_matches,
    ) {
        (2, true, false, false, _) => 0,
        (2, true, true, true, true) => 1,
        _ => 2,
    }
}

impl Compiled {
    pub(crate) fn finalize(&mut self) -> Result<()> {
        self.report["serialized_bytes"] = serde_json::json!(0);
        for _ in 0..4 {
            let size = serde_json::to_vec(&self.report)?.len();
            if self.report["serialized_bytes"] == size {
                break;
            }
            self.report["serialized_bytes"] = serde_json::json!(size);
        }
        let size = serde_json::to_vec(&self.report)?.len();
        self.report["serialized_bytes"] = serde_json::json!(size);
        ensure!(
            disclose::agent_intent_admitted(
                self.needs,
                self.sections,
                0,
                self.call_limit,
                size,
                self.packet_limit,
                true,
                true,
                true,
            ),
            "native agent intent packet exceeds its final admission policy."
        );
        Ok(())
    }
}
