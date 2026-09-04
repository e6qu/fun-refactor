use crate::edit::{Edit, EditSet};
use crate::extract::Extractor;
use crate::lang::detect;
use crate::parse::Parsers;
use crate::span::Span;
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

#[derive(Debug)]
pub struct Sync {
    pub report: Report,
    pub edits: EditSet,
    sources: Vec<SourceHash>,
}

#[derive(Debug)]
struct SourceHash {
    path: PathBuf,
    symbol: String,
    hash: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureReport>,
    pub status: Status,
}

#[derive(Debug, Serialize)]
pub struct SignatureReport {
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
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

    pub fn stale_signatures(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| {
                anchor
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.status == Status::Stale)
            })
            .count()
    }

    pub fn missing_signatures(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| {
                anchor
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.status == Status::Missing)
            })
            .count()
    }

    pub fn ok(&self) -> bool {
        self.stale() == 0
            && self.missing() == 0
            && self.anchors.iter().all(|anchor| {
                anchor
                    .signature
                    .as_ref()
                    .is_none_or(|signature| signature.status == Status::Fresh)
            })
    }
}

pub fn check(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Report> {
    check_with(root, inputs, respect_ignore, false)
}

pub fn check_strict(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Report> {
    check_with(root, inputs, respect_ignore, true)
}

fn check_with(
    root: &Path,
    inputs: &[PathBuf],
    respect_ignore: bool,
    require_signatures: bool,
) -> Result<Report> {
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
                    signature: None,
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
                        signature: None,
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
                        signature: None,
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
                        signature: None,
                        status: Status::Missing,
                    },
                }
            };
            let mut report = report;
            match signature_mapping(&text, line) {
                Ok(Some(mapping)) => match mapped_signature(&report, &text, line, &mapping) {
                    Ok(()) => {
                        report.signature = Some(SignatureReport {
                            status: Status::Fresh,
                            detail: None,
                        })
                    }
                    Err(error) => {
                        report.signature = Some(SignatureReport {
                            status: Status::Stale,
                            detail: Some(error.to_string()),
                        })
                    }
                },
                Ok(None) if require_signatures => {
                    report.signature = Some(SignatureReport {
                        status: Status::Missing,
                        detail: Some(
                            "the strict check requires an explicit signature map".to_string(),
                        ),
                    })
                }
                Ok(None) => {}
                Err(error) => {
                    report.signature = Some(SignatureReport {
                        status: Status::Missing,
                        detail: Some(error.to_string()),
                    })
                }
            }
            anchors.push(report);
        }
    }
    anchors.sort_by(|left, right| (&left.spec, left.line).cmp(&(&right.spec, right.line)));
    Ok(Report {
        anchors,
        obligations,
    })
}

pub fn sync(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Sync> {
    let report = check(root, inputs, respect_ignore)?;
    let mut edits = EditSet::new();
    let mut sources = Vec::new();
    for anchor in report
        .anchors
        .iter()
        .filter(|anchor| anchor.status == Status::Stale)
    {
        let text = crate::vfs::read_to_string(&anchor.spec)
            .with_context(|| format!("reading {}", anchor.spec.display()))?;
        let record = anchor_records(&text)?
            .into_iter()
            .find(|record| record.line == anchor.line)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{}:{} lost its spec anchor",
                    anchor.spec.display(),
                    anchor.line
                )
            })?;
        let actual = anchor.actual.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "{}:{} has no source hash to renew",
                anchor.spec.display(),
                anchor.line
            )
        })?;
        sources.push(SourceHash {
            path: anchor.source.clone(),
            symbol: anchor.symbol.clone(),
            hash: actual.to_string(),
        });
        edits.declare_language(&anchor.spec, crate::lang::Language::Lean);
        edits.add(
            &anchor.spec,
            Edit::new(
                record.hash_span,
                actual,
                format!(
                    "renew the source hash for {}::{}",
                    anchor.source.display(),
                    anchor.symbol
                ),
            ),
        );
    }
    Ok(Sync {
        report,
        edits,
        sources,
    })
}

impl Sync {
    pub fn verify_sources(&self) -> Result<()> {
        let mut parsers = Parsers::new();
        let mut extractor = Extractor::new();
        for source in &self.sources {
            let actual =
                declaration_hash(&mut parsers, &mut extractor, &source.path, &source.symbol)?;
            if actual != source.hash {
                bail!(
                    "{}::{} changed after spec sync planned it. Nothing written; re-run against the current source.",
                    source.path.display(),
                    source.symbol
                );
            }
        }
        Ok(())
    }
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
    anchor_records(text).map(|anchors| {
        anchors
            .into_iter()
            .map(|anchor| (anchor.line, anchor.source, anchor.symbol, anchor.expected))
            .collect()
    })
}

struct Anchor {
    line: usize,
    source: PathBuf,
    symbol: String,
    expected: String,
    hash_span: Span,
}

fn anchor_records(text: &str) -> Result<Vec<Anchor>> {
    const PREFIX: &str = "-- fr:spec ";
    const SEPARATOR: &str = " @ ";
    let mut anchors = Vec::new();
    let mut start = 0;
    for (number, chunk) in text.split_inclusive('\n').enumerate() {
        let line = chunk
            .strip_suffix('\n')
            .unwrap_or(chunk)
            .trim_end_matches('\r');
        let trimmed = line.trim_start();
        let Some(body) = trimmed.strip_prefix(PREFIX) else {
            start += chunk.len();
            continue;
        };
        let number = number + 1;
        let (target, expected) = body
            .split_once(SEPARATOR)
            .ok_or_else(|| anyhow::anyhow!("line {number}: a spec anchor needs ` @ <hash>`"))?;
        let (source, symbol) = target.rsplit_once("::").ok_or_else(|| {
            anyhow::anyhow!("line {number}: a spec anchor needs `<path>::<symbol>`")
        })?;
        if source.is_empty()
            || symbol.is_empty()
            || expected.len() < 8
            || expected.len() > 64
            || !expected.chars().all(|c| c.is_ascii_hexdigit())
        {
            bail!("line {number}: a spec anchor needs a path, symbol and hexadecimal hash");
        }
        let indentation = line.len() - trimmed.len();
        let hash_start = start + indentation + PREFIX.len() + target.len() + SEPARATOR.len();
        anchors.push(Anchor {
            line: number,
            source: PathBuf::from(source),
            symbol: symbol.to_string(),
            expected: expected.to_string(),
            hash_span: Span::new(hash_start, hash_start + expected.len()),
        });
        start += chunk.len();
    }
    Ok(anchors)
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignaturePart {
    name: String,
    ty: String,
}

fn signature_mapping(
    text: &str,
    anchor_line: usize,
) -> Result<Option<Vec<(SignaturePart, SignaturePart)>>> {
    let Some(line) = text.lines().nth(anchor_line) else {
        return Ok(None);
    };
    let Some(body) = line.trim_start().strip_prefix("-- fr:signature ") else {
        return Ok(None);
    };
    body.split(';')
        .map(|part| {
            let (source, model) = part.trim().split_once(" => ").ok_or_else(|| {
                anyhow::anyhow!(
                    "line {}: a signature map needs `source: Type => model: Type`",
                    anchor_line + 1
                )
            })?;
            Ok((signature_part(source)?, signature_part(model)?))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn signature_part(text: &str) -> Result<SignaturePart> {
    let (name, ty) = text
        .trim()
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("`{text}` needs a name and type"))?;
    if name.trim().is_empty() || ty.trim().is_empty() {
        bail!("`{text}` needs a name and type");
    }
    Ok(SignaturePart {
        name: name.trim().to_string(),
        ty: compact_type(ty),
    })
}

fn mapped_signature(
    anchor: &AnchorReport,
    spec: &str,
    anchor_line: usize,
    mapping: &[(SignaturePart, SignaturePart)],
) -> Result<()> {
    let source = rust_signature(&anchor.source, &anchor.symbol)?;
    let model = lean_signature(spec, anchor_line)?;
    let expected_source = mapping.iter().map(|(source, _)| source).collect::<Vec<_>>();
    let expected_model = mapping.iter().map(|(_, model)| model).collect::<Vec<_>>();
    if source.iter().collect::<Vec<_>>() != expected_source {
        bail!("the Rust signature no longer matches its explicit map");
    }
    if model.iter().collect::<Vec<_>>() != expected_model {
        bail!("the Lean declaration no longer matches its explicit map");
    }
    Ok(())
}

fn rust_signature(path: &Path, wanted: &str) -> Result<Vec<SignaturePart>> {
    if path.extension().is_none_or(|extension| extension != "rs") {
        bail!("explicit signature maps currently require a Rust source declaration");
    }
    let text = crate::vfs::read_to_string(path)?;
    let parsers = Parsers::new();
    let mut extractor = Extractor::new();
    let parsed = parsers.parse(crate::lang::Language::Rust, &text)?;
    let facts = extractor.extract(&parsed, path, &text)?;
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
    let declaration = symbol.full_span.text(&text);
    let open = declaration
        .find('(')
        .context("the Rust declaration has no parameters")?;
    let close = matching_delimiter(declaration, open, '(', ')')
        .context("the Rust declaration has no closing parameter list")?;
    let mut parts = declaration[open + 1..close]
        .split_top_level(',')
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .map(signature_part)
        .collect::<Result<Vec<_>>>()?;
    let tail = &declaration[close + 1..];
    let return_type = tail
        .split_once("->")
        .map(|(_, ty)| ty.split('{').next().unwrap_or(ty).trim())
        .unwrap_or("()");
    parts.push(SignaturePart {
        name: "return".to_string(),
        ty: compact_type(return_type),
    });
    Ok(parts)
}

fn lean_signature(spec: &str, anchor_line: usize) -> Result<Vec<SignaturePart>> {
    let mut header = String::new();
    let mut found = false;
    for line in spec.lines().skip(anchor_line) {
        if !found && !line.trim_start().starts_with("def ") {
            continue;
        }
        found = true;
        header.push_str(line.trim());
        header.push(' ');
        if line.contains(":=") {
            break;
        }
    }
    if !found {
        bail!("the signature map needs a following Lean definition");
    }
    let before_body = header
        .split_once(":=")
        .map(|(head, _)| head)
        .context("the mapped Lean definition needs `:=` on its declaration line")?;
    let mut parts = Vec::new();
    let mut rest = before_body
        .strip_prefix("def ")
        .context("a Lean definition starts with `def`")?;
    rest = rest
        .split_once(char::is_whitespace)
        .map(|(_, tail)| tail)
        .unwrap_or("");
    while !rest.trim_start().starts_with(':') {
        let open = rest
            .find('(')
            .context("a mapped Lean parameter needs `(`")?;
        let close = matching_delimiter(rest, open, '(', ')')
            .context("an explicit Lean parameter needs `)`")?;
        parts.push(signature_part(&rest[open + 1..close])?);
        rest = &rest[close + 1..];
    }
    let return_type = rest
        .split_once(':')
        .map(|(_, ty)| ty)
        .context("the mapped Lean definition needs a return type")?;
    parts.push(SignaturePart {
        name: "return".to_string(),
        ty: compact_type(return_type),
    });
    Ok(parts)
}

trait SplitTopLevel {
    fn split_top_level(&self, separator: char) -> Vec<&str>;
}

impl SplitTopLevel for str {
    fn split_top_level(&self, separator: char) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut depth: usize = 0;
        for (offset, character) in self.char_indices() {
            match character {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth = depth.saturating_sub(1),
                _ if character == separator && depth == 0 => {
                    parts.push(&self[start..offset]);
                    start = offset + character.len_utf8();
                }
                _ => {}
            }
        }
        parts.push(&self[start..]);
        parts
    }
}

fn matching_delimiter(text: &str, open: usize, left: char, right: char) -> Option<usize> {
    let mut depth = 0;
    for (offset, character) in text[open..].char_indices() {
        match character {
            character if character == left => depth += 1,
            character if character == right => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn compact_type(text: &str) -> String {
    text.split_whitespace().collect()
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
    use super::{anchors_in, check, check_strict, declaration_hash, obligations_in, sync, Status};
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

    #[test]
    fn sync_renews_stale_hashes_and_leaves_missing_anchors_unplanned() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(&code, "pub fn current() -> usize { 2 }\n").unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        let spec = root.join("specs/code.lean");
        fs::write(
            &spec,
            "-- fr:spec src/code.rs::current @ deadbeef\ndef current : Nat := 2\n",
        )
        .unwrap();

        let planned = sync(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(planned.report.stale(), 1);
        assert_eq!(planned.edits.file_count(), 1);
        let outcome = crate::edit::plan(&planned.edits, crate::edit::Validation::ReparseStrict)
            .unwrap()
            .pop()
            .unwrap();
        assert!(outcome.updated.contains(&hash));
        fs::write(&spec, outcome.updated).unwrap();
        assert!(check(root, &[PathBuf::from("specs")], true).unwrap().ok());

        fs::write(
            &spec,
            "-- fr:spec src/code.rs::current @ deadbeef\ndef current : Nat := 2\n",
        )
        .unwrap();
        let stale_source = sync(root, &[PathBuf::from("specs")], true).unwrap();
        fs::write(&code, "pub fn current() -> usize { 3 }\n").unwrap();
        assert!(stale_source.verify_sources().is_err());

        fs::write(
            &spec,
            "-- fr:spec src/code.rs::gone @ deadbeef\ndef gone : Nat := 0\n",
        )
        .unwrap();
        let missing = sync(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(missing.report.missing(), 1);
        assert!(missing.edits.is_empty());
    }

    #[test]
    fn checks_an_explicit_rust_to_lean_signature_map() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(
            &code,
            "pub fn current(source: &str, offset: usize) -> String { source.into() }\n",
        )
        .unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        let spec = root.join("specs/code.lean");
        fs::write(
            &spec,
            format!(
                "-- fr:spec src/code.rs::current @ {}\n-- fr:signature source: &str => source: String; offset: usize => offset: Nat; return: String => return: String\ndef current (source : String) (offset : Nat) : String := source\n-- end.\n",
                &hash[..8]
            ),
        )
        .unwrap();

        let fresh = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(
            fresh.anchors[0].signature.as_ref().unwrap().status,
            Status::Fresh
        );
        assert!(fresh.ok());

        fs::write(
            &spec,
            format!(
                "-- fr:spec src/code.rs::current @ {}\n-- fr:signature source: &str => source: String; offset: usize => offset: Nat; return: String => return: Nat\ndef current (source : String) (offset : Nat) : String := source\n-- end.\n",
                &hash[..8]
            ),
        )
        .unwrap();
        let stale = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(
            stale.anchors[0].signature.as_ref().unwrap().status,
            Status::Stale
        );
        assert!(!stale.ok());
    }

    #[test]
    fn strict_check_requires_every_anchor_to_map_its_signature() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(&code, "pub fn current() -> usize { 1 }\n").unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        fs::write(
            root.join("specs/code.lean"),
            format!(
                "-- fr:spec src/code.rs::current @ {}\ndef current : Nat := 1\n",
                &hash[..8]
            ),
        )
        .unwrap();
        assert!(check(root, &[PathBuf::from("specs")], true).unwrap().ok());
        let strict = check_strict(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(strict.missing_signatures(), 1);
        assert!(!strict.ok());
    }

    #[test]
    fn checks_nested_rust_types_against_a_multiline_lean_definition() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(
            &code,
            "pub fn current(source: Vec<(String, usize)>, callback: impl Fn(&str, usize) -> String) -> Result<(String, usize), ()> { unimplemented!() }\n",
        )
        .unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        fs::write(
            root.join("specs/code.lean"),
            format!(
                "-- fr:spec src/code.rs::current @ {}\n-- fr:signature source: Vec<(String, usize)> => source: List (String × Nat); callback: impl Fn(&str, usize) -> String => callback: String → Nat → String; return: Result<(String, usize), ()> => return: Option (String × Nat)\ndef current\n    (source : List (String × Nat))\n    (callback : String → Nat → String)\n    : Option (String × Nat) := none\n",
                &hash[..8]
            ),
        )
        .unwrap();
        let report = check_strict(root, &[PathBuf::from("specs")], true).unwrap();
        assert!(report.ok(), "{report:#?}");
    }
}
