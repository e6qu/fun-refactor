use crate::extract::Extractor;
use crate::lang::detect;
use crate::parse::Parsers;
use anyhow::{bail, Context, Result};
use ignore::WalkBuilder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Report {
    pub anchors: Vec<AnchorReport>,
    pub obligations: usize,
}

#[derive(Debug, Serialize)]
pub struct AnchorReport {
    pub spec: PathBuf,
    pub line: usize,
    pub source: PathBuf,
    pub symbol: String,
    pub expected: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Fresh,
    Stale,
    Missing,
}

impl Report {
    pub fn fresh(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| anchor.status == Status::Fresh)
            .count()
    }

    pub fn stale(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| anchor.status == Status::Stale)
            .count()
    }

    pub fn missing(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| anchor.status == Status::Missing)
            .count()
    }

    pub fn ok(&self) -> bool {
        self.stale() == 0 && self.missing() == 0
    }
}

pub fn check(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Report> {
    let files = spec_files(root, inputs, respect_ignore)?;
    let mut anchors = Vec::new();
    let mut obligations = 0;
    let mut parsers = Parsers::new();
    let mut extractor = Extractor::new();

    for spec in files {
        let text = crate::vfs::read_to_string(&spec)
            .with_context(|| format!("reading {}", spec.display()))?;
        obligations += obligations_in(&text);
        for (line, source, symbol, expected) in anchors_in(&text)? {
            let source = crate::vfs::normalise(root.join(source));
            let report = if !source.starts_with(root) {
                AnchorReport {
                    spec: spec.clone(),
                    line,
                    source,
                    symbol,
                    expected,
                    actual: None,
                    detail: Some("a spec anchor may not leave the workspace".to_string()),
                    status: Status::Missing,
                }
            } else {
                match declaration_hash(&mut parsers, &mut extractor, &source, &symbol) {
                    Ok(actual) if actual.starts_with(&expected) => AnchorReport {
                        spec: spec.clone(),
                        line,
                        source,
                        symbol,
                        expected,
                        actual: Some(actual),
                        detail: None,
                        status: Status::Fresh,
                    },
                    Ok(actual) => AnchorReport {
                        spec: spec.clone(),
                        line,
                        source,
                        symbol,
                        expected,
                        actual: Some(actual),
                        detail: None,
                        status: Status::Stale,
                    },
                    Err(error) => AnchorReport {
                        spec: spec.clone(),
                        line,
                        source,
                        symbol,
                        expected,
                        actual: None,
                        detail: Some(error.to_string()),
                        status: Status::Missing,
                    },
                }
            };
            anchors.push(report);
        }
    }
    anchors.sort_by(|left, right| (&left.spec, left.line).cmp(&(&right.spec, right.line)));
    Ok(Report {
        anchors,
        obligations,
    })
}

fn spec_files(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Vec<PathBuf>> {
    let inputs = match inputs.is_empty() {
        true => [root.join("kernels"), root.join("specs")]
            .into_iter()
            .filter(|path| crate::vfs::exists(path))
            .collect::<Vec<_>>(),
        false => inputs
            .iter()
            .map(|input| match input.is_absolute() {
                true => input.clone(),
                false => root.join(input),
            })
            .collect(),
    };
    let mut files = BTreeSet::new();
    for input in inputs {
        let metadata =
            std::fs::metadata(&input).with_context(|| format!("reading {}", input.display()))?;
        if metadata.is_file() {
            lean_file(&input)?;
            files.insert(input);
            continue;
        }
        let mut found = false;
        let walker = WalkBuilder::new(&input)
            .standard_filters(respect_ignore)
            .hidden(respect_ignore)
            .git_ignore(respect_ignore)
            .require_git(false)
            .build();
        for entry in walker {
            let entry = entry?;
            if entry.file_type().is_some_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "lean")
            {
                found = true;
                files.insert(entry.into_path());
            }
        }
        if !found {
            bail!("{} contains no .lean specs", input.display());
        }
    }
    if files.is_empty() {
        bail!("no Lean spec roots found; name a .lean file or directory");
    }
    Ok(files.into_iter().collect())
}

fn lean_file(path: &Path) -> Result<()> {
    if path.extension().is_none_or(|extension| extension != "lean") {
        bail!("{} is not a .lean spec", path.display());
    }
    Ok(())
}

fn anchors_in(text: &str) -> Result<Vec<(usize, PathBuf, String, String)>> {
    text.lines()
        .enumerate()
        .filter_map(|(number, line)| {
            line.trim_start()
                .strip_prefix("-- fr:spec ")
                .map(|body| (number + 1, body))
        })
        .map(|(line, body)| {
            let (target, expected) = body
                .split_once(" @ ")
                .ok_or_else(|| anyhow::anyhow!("line {line}: a spec anchor needs ` @ <hash>`"))?;
            let (source, symbol) = target.rsplit_once("::").ok_or_else(|| {
                anyhow::anyhow!("line {line}: a spec anchor needs `<path>::<symbol>`")
            })?;
            if source.is_empty()
                || symbol.is_empty()
                || expected.len() < 8
                || expected.len() > 64
                || !expected.chars().all(|c| c.is_ascii_hexdigit())
            {
                bail!("line {line}: a spec anchor needs a path, symbol and hexadecimal hash");
            }
            Ok((
                line,
                PathBuf::from(source),
                symbol.to_string(),
                expected.to_string(),
            ))
        })
        .collect()
}

fn declaration_hash(
    parsers: &mut Parsers,
    extractor: &mut Extractor,
    path: &Path,
    wanted: &str,
) -> Result<String> {
    let language = detect(path)
        .ok_or_else(|| anyhow::anyhow!("{} has no language this build reads", path.display()))?;
    let source =
        crate::vfs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed = parsers.parse(language, &source)?;
    let facts = extractor.extract(&parsed, path, &source)?;
    let matches = facts
        .symbols
        .iter()
        .filter(|symbol| symbol.qualified_name() == wanted)
        .collect::<Vec<_>>();
    let [symbol] = matches.as_slice() else {
        bail!(
            "{} names {} declarations called {wanted}",
            path.display(),
            matches.len()
        );
    };
    Ok(format!(
        "{:x}",
        Sha256::digest(symbol.full_span.text(&source).as_bytes())
    ))
}

fn obligations_in(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .flat_map(|line| line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
        .filter(|word| *word == "sorry")
        .count()
}

#[cfg(test)]
mod tests {
    use super::{anchors_in, check, declaration_hash, obligations_in, Status};
    use crate::extract::Extractor;
    use crate::parse::Parsers;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn reads_anchors_and_counts_only_live_obligations() {
        let anchors = anchors_in(
            "-- fr:spec src/edit.rs::apply_to_string @ deadbeef\ndef x := sorry\n-- sorry\n",
        )
        .unwrap();
        assert_eq!(anchors[0].0, 1);
        assert_eq!(anchors[0].1.to_string_lossy(), "src/edit.rs");
        assert_eq!(anchors[0].2, "apply_to_string");
        assert_eq!(obligations_in("def x := sorry\n-- sorry\n"), 1);
    }

    #[test]
    fn reports_fresh_stale_and_missing_anchors() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let source = "pub fn current() -> usize { 1 }\n";
        let code = root.join("src/code.rs");
        fs::write(&code, source).unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        fs::write(
            root.join("specs/code.lean"),
            format!(
                "-- fr:spec src/code.rs::current @ {}\ndef current : Nat := sorry\n-- sorry\n",
                &hash[..8]
            ),
        )
        .unwrap();

        let fresh = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(fresh.anchors[0].status, Status::Fresh);
        assert_eq!(fresh.obligations, 1);

        fs::write(&code, "pub fn current() -> usize { 2 }\n").unwrap();
        let stale = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(stale.anchors[0].status, Status::Stale);

        fs::write(
            root.join("specs/code.lean"),
            "-- fr:spec src/code.rs::gone @ deadbeef\ndef gone : Nat := 0\n",
        )
        .unwrap();
        let missing = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(missing.anchors[0].status, Status::Missing);

        fs::write(
            root.join("specs/code.lean"),
            "-- fr:spec ../outside.rs::gone @ deadbeef\ndef gone : Nat := 0\n",
        )
        .unwrap();
        let outside = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(outside.anchors[0].status, Status::Missing);
        assert!(outside.anchors[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("may not leave")));
    }
}
