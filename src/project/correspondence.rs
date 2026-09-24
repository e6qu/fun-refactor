use super::{hash, page, Project};
use crate::project::occurrence::Occurrence;
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const CONTRACT: &str = "fr-declaration-identities-2";

#[derive(Args)]
pub struct Options {
    #[arg(default_value = ".")]
    target: String,
    #[arg(long)]
    from: Option<PathBuf>,
    #[arg(long, requires = "from")]
    digest: Option<String>,
    #[arg(long, default_value_t = 40)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long, default_value_t = 16)]
    candidates: usize,
    #[arg(long, default_value_t = 32768)]
    bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub handle: String,
    pub revision: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub language: String,
    pub scope: Vec<String>,
    pub object_digest: String,
    pub name_erased_digest: String,
    pub occurrence: Occurrence,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: String,
    revision: String,
    selection: String,
    analyzer: String,
    complete: bool,
    items: Vec<Identity>,
}

pub fn correspondence_status(candidates: usize, shared: bool) -> &'static str {
    match (candidates, shared) {
        (0, _) => "missing",
        (1, false) => "matched",
        _ => "ambiguous",
    }
}

pub fn analyzer_digest() -> Result<String> {
    hash((CONTRACT, include_str!("correspondence.rs")))
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn validate(items: &[Identity], revision: &str) -> Result<()> {
    ensure!(digest_valid(revision), "invalid identity revision");
    ensure!(items.len() <= 1000, "too many correspondence inputs");
    let mut handles = BTreeSet::new();
    for item in items {
        let suffix = item
            .handle
            .strip_prefix(&format!("frp1:{}:", &revision[..32]));
        ensure!(
            item.revision == revision
                && suffix
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()))
                && handles.insert(&item.handle),
            "duplicate or inconsistent declaration handle."
        );
        ensure!(
            digest_valid(&item.object_digest) && digest_valid(&item.name_erased_digest),
            "invalid declaration digest"
        );
        let origin = &item.occurrence;
        ensure!(
            origin.revision == revision
                && origin.path == item.path
                && origin.role == "declaration"
                && origin.enclosing.as_deref() == Some(&item.handle)
                && origin.location.span.start <= origin.location.span.end
                && origin.id
                    == format!(
                        "fro1:{}",
                        hash((revision, &item.path, origin.location.span, "declaration"))?
                    ),
            "inconsistent declaration occurrence"
        );
        ensure!(
            !item.name.is_empty()
                && !item.kind.is_empty()
                && !item.language.is_empty()
                && !std::path::Path::new(&item.path).is_absolute()
                && std::path::Path::new(&item.path)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
            "invalid declaration metadata"
        );
    }
    Ok(())
}

fn previous(value: &Value) -> Result<(Vec<Identity>, bool)> {
    let (items, revision, complete) =
        if value["schema"] == "fr-declaration-snapshot-1" {
            let snapshot: Snapshot = serde_json::from_value(value.clone())?;
            ensure!(
                snapshot.schema == "fr-declaration-snapshot-1"
                    && !snapshot.selection.is_empty()
                    && digest_valid(&snapshot.analyzer),
                "invalid declaration snapshot"
            );
            (snapshot.items, snapshot.revision, snapshot.complete)
        } else {
            ensure!(value["schema"] == "fr-project-1" && value["query"] == "identities"
            && value["identity_contract"] == CONTRACT,
            "expected version 2 identity report or declaration snapshot; recapture older reports.");
            let items: Vec<Identity> = serde_json::from_value(value["items"].clone())?;
            let p = &value["page"];
            ensure!(
                p["returned"].as_u64() == Some(items.len() as u64)
                    && p["before"]
                        .as_u64()
                        .zip(p["remaining"].as_u64())
                        .is_some_and(|(a, b)| a
                            .checked_add(b)
                            .and_then(|n| n.checked_add(items.len() as u64))
                            == p["total"].as_u64()),
                "inconsistent identity page"
            );
            let complete = p["before"] == 0 && p["remaining"] == 0;
            ensure!(
                value["complete"] == complete,
                "inconsistent identity coverage"
            );
            (
                items,
                value["revision"]
                    .as_str()
                    .context("missing identity revision")?
                    .to_owned(),
                complete,
            )
        };
    validate(&items, &revision)?;
    Ok((items, complete))
}

fn rules(old: &Identity, new: &Identity) -> Vec<&'static str> {
    let mut rules = Vec::new();
    if old.object_digest == new.object_digest {
        rules.push("identical-declaration-content");
    }
    if old.name_erased_digest == new.name_erased_digest {
        rules.push("identical-except-declaration-name");
    }
    if old.path == new.path
        && old.name == new.name
        && old.kind == new.kind
        && old.language == new.language
        && old.scope == new.scope
    {
        rules.push("same-path-scope-name-kind");
    }
    rules
}

impl Project<'_> {
    fn declaration_identities(&self, selected: usize) -> Result<Vec<Identity>> {
        let mut identities = Vec::new();
        for (id, node) in self.nodes.iter().enumerate() {
            if !self.within(id, selected) {
                continue;
            }
            let Some(symbol) = node.symbol.and_then(|id| self.index.symbol(id)) else {
                continue;
            };
            let (source, span) = self.source(id)?;
            ensure!(
                span.contains(symbol.name_span),
                "declaration name leaves its source span"
            );
            let mut scope = Vec::new();
            let mut parent = node.parent;
            while let Some(id) = parent {
                let node = &self.nodes[id];
                if let Some(symbol) = node.symbol.and_then(|id| self.index.symbol(id)) {
                    scope.push(format!("{}:{}", symbol.kind.as_str(), symbol.name));
                }
                parent = node.parent;
            }
            scope.reverse();
            let mut occurrence = self.occurrence(&symbol.file, span, "declaration")?;
            occurrence.enclosing = Some(self.handle(id));
            identities.push(Identity {
                handle: self.handle(id),
                revision: self.revision.clone(),
                path: node.path.to_string_lossy().into_owned(),
                name: symbol.name.clone(),
                kind: symbol.kind.as_str().into(),
                language: symbol.language.to_string(),
                scope,
                object_digest: hash((
                    "fr-declaration-object-1",
                    symbol.language,
                    symbol.kind,
                    span.text(source),
                ))?,
                name_erased_digest: hash((
                    "fr-declaration-name-erased-1",
                    symbol.language,
                    symbol.kind,
                    &source[span.start..symbol.name_span.start],
                    &source[symbol.name_span.end..span.end],
                ))?,
                occurrence,
            });
        }
        identities
            .sort_by(|a, b| (&a.path, &a.name, &a.handle).cmp(&(&b.path, &b.name, &b.handle)));
        Ok(identities)
    }

    pub(super) fn identities(&self, options: &Options) -> Result<Value> {
        ensure!(
            (1..=64).contains(&options.candidates),
            "candidate limit must be 1..64"
        );
        ensure!(
            (4096..=1_048_576).contains(&options.bytes),
            "identity byte budget must be 4096..1048576."
        );
        let selected = self.target(&options.target)?;
        let identities = self.declaration_identities(selected)?;
        let analyzer = analyzer_digest()?;
        let mut input_complete = true;
        let mut input_digest = None;
        let rows = if let Some(path) = &options.from {
            ensure!(
                std::fs::metadata(path)?.len() <= 1_048_576,
                "identity report exceeds 1 MiB"
            );
            let value: Value = serde_json::from_slice(&crate::vfs::read(path)?)?;
            let digest = hash(&value)?;
            ensure!(
                options.digest.as_ref().is_none_or(|d| d == &digest),
                "identity report digest mismatch"
            );
            input_digest = Some(digest);
            let (old, complete) = previous(&value)?;
            input_complete = complete;
            let matches: Vec<Vec<_>> = old
                .iter()
                .map(|old| {
                    identities
                        .iter()
                        .filter_map(|new| {
                            let reasons = rules(old, new);
                            (!reasons.is_empty()).then_some((new, reasons))
                        })
                        .collect()
                })
                .collect();
            let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for (old, candidates) in old.iter().zip(&matches) {
                for (new, _) in candidates {
                    owners.entry(&new.handle).or_default().push(&old.handle);
                }
            }
            old.iter().zip(matches.iter()).map(|(old, candidates)| {
                let conflicts: BTreeSet<_> = candidates.iter().flat_map(|(new, _)| owners[new.handle.as_str()].iter().copied())
                    .filter(|handle| *handle != old.handle).collect();
                let shown: Vec<_> = candidates.iter().take(options.candidates).collect();
                json!({"previous": old, "status": correspondence_status(candidates.len(), !conflicts.is_empty()),
                    "candidates": shown.iter().map(|(new, _)| new).collect::<Vec<_>>(),
                    "reasons": shown.iter().map(|(new, reasons)| json!({"handle":new.handle,"rules":reasons,
                        "content_equal":new.object_digest == old.object_digest})).collect::<Vec<_>>(),
                    "candidate_count": candidates.len(), "candidates_complete": shown.len() == candidates.len(),
                    "conflicts": conflicts, "action_rebound": false})
            }).collect::<Vec<_>>()
        } else {
            identities.iter().map(|item| json!(item)).collect()
        };
        let key = format!(
            "frpc1:{}",
            &hash((
                &self.revision,
                &analyzer,
                selected,
                &input_digest,
                options.candidates,
                options.bytes
            ))?[..32]
        );
        let (start, mut end, _) = page(rows.len(), options.limit, options.cursor.as_deref(), &key)?;
        let mut report = self.envelope(if options.from.is_some() {
            "correspondence"
        } else {
            "identities"
        });
        report["identity_contract"] = json!(CONTRACT);
        report["analyzer"] = json!(analyzer);
        report["selection"] = json!(options.target);
        report["input_digest"] = json!(input_digest);
        report["input_complete"] = json!(input_complete);
        report["basis"] = json!(key);
        report["limits"] =
            json!({"bytes": options.bytes, "candidates":options.candidates, "limit":options.limit});
        report["claim"] = json!("syntactic candidates among indexed declarations in the selected scope; no semantic equivalence or mutation authority.");
        report["graph_policy"] = json!("declaration content excludes graph edges; occurrence handles identify the current revision.");
        loop {
            report["items"] = json!(&rows[start..end]);
            report["page"] = json!({"total":rows.len(),"before":start,"returned":end-start,"remaining":rows.len()-end,
                "next":(end < rows.len()).then(|| format!("{key}:{end}"))});
            report["complete"] = json!(
                input_complete
                    && start == 0
                    && end == rows.len()
                    && rows[start..end]
                        .iter()
                        .all(|r| r.get("candidates_complete").is_none_or(|v| v == true))
            );
            if serde_json::to_vec(&report)?.len() <= options.bytes {
                break;
            }
            ensure!(
                end > start + 1,
                "one identity row exceeds byte budget; increase --bytes or reduce --candidates."
            );
            end -= 1;
        }
        Ok(report)
    }
}
