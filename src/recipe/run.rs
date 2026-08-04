//! Running a recipe: select, act, re-index, and say what happened.
//!
//! A recipe is **one transaction**. Every step's edits are applied to an in-memory
//! copy of the workspace and the index is rebuilt from it, so each step sees what the
//! previous one left; nothing reaches disk until the whole run has succeeded. A
//! half-applied recipe leaves a repository in a state nobody designed — the flag
//! removed and its dead branches still there.

use super::parse::{Expect, OnRefusal, Operation, Predicate, Recipe, Requirement, Step};
use crate::analysis::entrypoints::Entrypoints;
use crate::edit::EditSet;
use crate::index::Index;
use crate::lang::Language;
use crate::model::{SymbolId, SymbolKind};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The workspace as the run currently believes it to be.
type Sources = BTreeMap<PathBuf, (Language, String)>;

#[derive(Debug, Serialize)]
pub struct Report {
    pub recipe: String,
    pub description: Option<String>,
    pub steps: Vec<StepReport>,
    pub expectations: Vec<ExpectReport>,
    /// Files whose text differs from where the run started.
    pub files_changed: usize,
    /// Did every expectation hold, and was every refusal permitted?
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct StepReport {
    pub step: String,
    pub selector: String,
    pub matched: usize,
    pub applied: usize,
    pub refusals: Vec<Refusal>,
    /// What the operation left alone and said so about.
    ///
    /// A refusal is the operation declining; a warning is it succeeding and telling
    /// you what it could not verify — a reference that resolved too weakly to rewrite,
    /// a name in a comment. Dropping these on the floor is the accept-and-ignore this
    /// codebase bans elsewhere: `fr rename` prints them and a recipe was swallowing
    /// them, so a step that left work behind reported a clean run.
    pub warnings: Vec<String>,
    pub files_changed: usize,
}

#[derive(Debug, Serialize)]
pub struct Refusal {
    pub subject: String,
    pub reason: String,
    /// True when `on-refusal allow` said these were expected.
    pub permitted: bool,
}

#[derive(Debug, Serialize)]
pub struct ExpectReport {
    pub expectation: String,
    pub actual: String,
    pub held: bool,
}

/// What the caller wants done with the result.
pub struct Options<'a> {
    pub root: &'a Path,
    /// Extra entry-point catalogue directories, as `fr unused` takes.
    pub catalogs: &'a [PathBuf],
}

/// Run one recipe over a workspace, returning the report and the edits it would make.
pub fn run(recipe: &Recipe, sources: Sources, options: &Options) -> Result<(Report, Sources)> {
    let originals = sources.clone();
    let mut sources = sources;

    let mut index = reindex(&sources)?;
    check_requirements(recipe, &index, &sources)?;

    // Files this run has already touched, which is what `where changed` selects on.
    let mut changed: BTreeSet<PathBuf> = BTreeSet::new();
    let mut steps = Vec::new();
    let mut total_refusals = 0usize;
    let mut stopped = None;

    let before = analyses(&index, options)?;

    for step in &recipe.steps {
        let report = run_step(step, &index, &sources, &changed, options)?;

        // `stop` is the default because a step that refused has not done what the
        // recipe says it does, and the steps after it were written expecting it had.
        if step.on_refusal == OnRefusal::Stop && !report.report.refusals.is_empty() {
            let first = &report.report.refusals[0];
            stopped = Some(format!(
                "step {} refused on {}: {}",
                steps.len() + 1,
                first.subject,
                first.reason
            ));
            steps.push(report.report);
            break;
        }

        total_refusals += report.report.refusals.len();
        for path in report.edits.paths() {
            changed.insert(path.clone());
        }
        apply(&mut sources, &report.edits)?;
        index = reindex(&sources)?;
        steps.push(report.report);
    }

    let files_changed = sources
        .iter()
        .filter(|(path, (_, text))| originals.get(*path).map(|(_, before)| before) != Some(text))
        .count();

    let mut expectations = Vec::new();
    if stopped.is_none() {
        let after = analyses(&index, options)?;
        for expect in &recipe.expects {
            expectations.push(check_expect(
                expect,
                &before,
                &after,
                files_changed as u64,
                total_refusals as u64,
            ));
        }
    }

    let permitted = recipe
        .steps
        .iter()
        .all(|s| s.on_refusal != OnRefusal::Report)
        || total_refusals == 0;
    let ok = stopped.is_none() && expectations.iter().all(|e| e.held) && permitted;

    if let Some(why) = stopped {
        return Ok((
            Report {
                recipe: recipe.name.clone(),
                description: recipe.description.clone(),
                steps,
                expectations,
                files_changed: 0,
                ok: false,
            },
            // Nothing is written: the transaction did not complete.
            originals,
        ))
        .map(|(mut report, sources)| {
            report.steps.push(StepReport {
                step: format!("stopped: {why}"),
                selector: String::new(),
                matched: 0,
                applied: 0,
                refusals: Vec::new(),
                warnings: Vec::new(),
                files_changed: 0,
            });
            (report, sources)
        });
    }

    Ok((
        Report {
            recipe: recipe.name.clone(),
            description: recipe.description.clone(),
            steps,
            expectations,
            files_changed,
            ok,
        },
        sources,
    ))
}

/// Rebuild the index, and hand the same text to everything that reads a file.
///
/// The refactorings read source through [`crate::vfs`], not from this map, so without
/// installing it a plan made after one step is measured against the file on disk —
/// which is the text before *any* step ran.
fn reindex(sources: &Sources) -> Result<Index> {
    let handle = crate::vfs::new_handle(
        sources
            .iter()
            .map(|(path, (_, text))| (path.clone(), text.clone())),
    );
    crate::vfs::activate(&handle);
    let snapshot: Vec<(PathBuf, Language, String)> = sources
        .iter()
        .map(|(p, (l, s))| (p.clone(), *l, s.clone()))
        .collect();
    Index::build_from_sources(&snapshot)
}

fn apply(sources: &mut Sources, edits: &EditSet) -> Result<()> {
    for path in edits.paths() {
        let Some(list) = edits.edits_for(path) else {
            continue;
        };
        let entry = sources.entry(path.clone()).or_insert_with(|| {
            (
                crate::lang::detect(path).unwrap_or(Language::Markdown),
                String::new(),
            )
        });
        entry.1 = crate::edit::apply_to_string(&entry.1, list)
            .with_context(|| format!("applying edits to {}", path.display()))?;
    }
    Ok(())
}

fn check_requirements(recipe: &Recipe, index: &Index, sources: &Sources) -> Result<()> {
    for requirement in &recipe.requires {
        match requirement {
            Requirement::Language(name) => {
                let Some(language) = Language::from_name(name) else {
                    bail!("`requires language {name}` — no such language");
                };
                if !sources.values().any(|(l, _)| *l == language) {
                    bail!(
                        "`requires language {name}` — this workspace has no {name} file, so \
                         the recipe would do nothing and say it had succeeded"
                    );
                }
            }
            Requirement::Symbol(name) => {
                if !index.symbols.iter().any(|s| s.name == *name) {
                    bail!(
                        "`requires symbol \"{name}\"` — nothing in this workspace is called \
                         that. The recipe was written for a different tree."
                    );
                }
            }
            Requirement::Path(path) => {
                if !sources.keys().any(|p| p.to_string_lossy().contains(path)) {
                    bail!("`requires path \"{path}\"` — no file in this workspace is under it");
                }
            }
        }
    }
    Ok(())
}

/// The analyses an `expect no-new` compares against.
struct Analyses {
    unused: BTreeSet<String>,
    duplicates: usize,
}

fn analyses(index: &Index, options: &Options) -> Result<Analyses> {
    let mut catalog = crate::analysis::entrypoints::Catalog::builtin()?;
    for dir in options.catalogs {
        catalog.load_dir(dir)?;
    }
    let entrypoints = Entrypoints::from_catalog(&catalog, index);
    let unused = crate::refactor::delete::find_unused(index, &entrypoints)
        .into_iter()
        .filter_map(|id| index.symbol(id))
        .map(|s| format!("{}:{}", s.file.display(), s.name))
        .collect();
    let duplicates = crate::analysis::duplicates::find(index, &Default::default())
        .map(|classes| classes.len())
        .unwrap_or(0);
    Ok(Analyses { unused, duplicates })
}

fn check_expect(
    expect: &Expect,
    before: &Analyses,
    after: &Analyses,
    files_changed: u64,
    refusals: u64,
) -> ExpectReport {
    match expect {
        Expect::NoNew(what) => {
            let (count, detail) = match what.as_str() {
                "unused" => {
                    let fresh: Vec<&String> = after.unused.difference(&before.unused).collect();
                    (fresh.len() as u64, format!("{} new", fresh.len()))
                }
                _ => {
                    let fresh = after.duplicates.saturating_sub(before.duplicates);
                    (fresh as u64, format!("{fresh} new"))
                }
            };
            ExpectReport {
                expectation: format!("no-new {what}"),
                actual: detail,
                held: count == 0,
            }
        }
        Expect::Changed { how, count } => ExpectReport {
            expectation: format!("changed {} {count} files", how.as_str()),
            actual: format!("{files_changed} files"),
            held: how.holds(files_changed, *count),
        },
        Expect::Refusals { how, count } => ExpectReport {
            expectation: format!("refusals {} {count}", how.as_str()),
            actual: refusals.to_string(),
            held: how.holds(refusals, *count),
        },
    }
}

struct StepOutcome {
    report: StepReport,
    edits: EditSet,
}

fn run_step(
    step: &Step,
    index: &Index,
    sources: &Sources,
    changed: &BTreeSet<PathBuf>,
    options: &Options,
) -> Result<StepOutcome> {
    let selector = step
        .selector
        .iter()
        .map(|p| p.describe())
        .collect::<Vec<_>>()
        .join(" ");

    let mut edits = EditSet::new();
    let mut refusals = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut applied = 0usize;
    let permitted = step.on_refusal == OnRefusal::Allow;

    // The three workspace-wide operations select nothing: their target is the shape
    // they are given, and the signature table already rejected a `where` on them.
    let matched = match &step.operation {
        Operation::RemoveFlag { flag, value } => {
            match crate::refactor::cascade::remove_flag_in(sources.clone(), flag, *value) {
                Ok(plan) => {
                    merge(&mut edits, &plan.edits);
                    applied = 1;
                }
                Err(e) => refusals.push(Refusal {
                    subject: flag.clone(),
                    reason: e.to_string(),
                    permitted,
                }),
            }
            1
        }
        Operation::Restructure {
            language,
            pattern,
            template,
        } => {
            let Some(language) = Language::from_name(language) else {
                bail!("line {}: no language called '{language}'", step.line);
            };
            match crate::refactor::restructure::apply(index, language, pattern, template) {
                Ok(plan) => {
                    applied = plan.edits.file_count();
                    merge(&mut edits, &plan.edits);
                }
                Err(e) => refusals.push(Refusal {
                    subject: pattern.clone(),
                    reason: e.to_string(),
                    permitted,
                }),
            }
            1
        }
        Operation::ExtractVariable { at, name } | Operation::ExtractFunction { at, name } => {
            let function = matches!(step.operation, Operation::ExtractFunction { .. });
            match extract_at(index, options.root, at, name, function) {
                Ok(set) => {
                    applied = 1;
                    merge(&mut edits, &set);
                }
                Err(e) => refusals.push(Refusal {
                    subject: at.clone(),
                    reason: e.to_string(),
                    permitted,
                }),
            }
            1
        }
        // Everything else acts on the symbols, or the files, a selector chose.
        _ => {
            let chosen = select(step, index, changed, options)?;
            let total = chosen.len();
            let taken: Vec<Subject> = match step.limit {
                Some(limit) => chosen.into_iter().take(limit as usize).collect(),
                None => chosen,
            };

            // Each subject is planned against the workspace the previous one left.
            let mut running = sources.clone();
            let mut current = reindex(&running)?;
            for subject in taken {
                if matches!(subject, Subject::Symbol { .. }) && subject.resolve(&current).is_none()
                {
                    // Already gone — an earlier subject's cascade took it. That is the
                    // step succeeding, not failing.
                    continue;
                }
                match act(&step.operation, &current, &subject, options.root) {
                    Ok((set, said)) => {
                        applied += 1;
                        warnings.extend(said);
                        apply(&mut running, &set)?;
                        current = reindex(&running)?;
                    }
                    Err(e) => refusals.push(Refusal {
                        subject: subject.describe(),
                        reason: e.to_string(),
                        permitted,
                    }),
                }
            }

            // The step's edit is the difference it made, whole-file: the individual
            // spans were measured against intermediate text that no longer exists.
            for (path, (_, text)) in &running {
                let before = sources.get(path).map(|(_, t)| t.as_str()).unwrap_or("");
                if before != text {
                    edits.add(
                        path.clone(),
                        crate::edit::Edit::new(
                            crate::span::Span::new(0, before.len()),
                            text,
                            step.operation.describe(),
                        ),
                    );
                }
            }
            total
        }
    };

    // A selector that matches nothing stops the recipe. Silently doing nothing is the
    // failure this most wants to avoid, because it looks exactly like success.
    if matched == 0 && !step.allow_empty {
        bail!(
            "line {}: `{}` matched nothing. That is not success — write `allow-empty` if \
             this step is genuinely conditional.",
            step.line,
            step.operation.describe()
        );
    }

    Ok(StepOutcome {
        report: StepReport {
            step: step.operation.describe(),
            selector,
            matched,
            applied,
            files_changed: edits.file_count(),
            refusals,
            warnings,
        },
        edits,
    })
}

fn merge(into: &mut EditSet, from: &EditSet) {
    for path in from.paths() {
        if let Some(list) = from.edits_for(path) {
            for edit in list {
                into.add(path.clone(), edit.clone());
            }
        }
    }
}

/// What a step acts on: a symbol, or a whole file.
///
/// A symbol is named rather than identified. Each subject is acted on against a
/// freshly built index, because one deletion moves every span after it in the file and
/// a `SymbolId` does not survive a rebuild — planning them all against one snapshot
/// produced `conflicting edits: 0..396 overlaps 26..170` the first time two symbols in
/// one file were selected together.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    Symbol {
        file: PathBuf,
        name: String,
        kind: SymbolKind,
    },
    File(PathBuf),
}

impl Subject {
    fn describe(&self) -> String {
        match self {
            Subject::Symbol { file, name, .. } => format!("{name} ({})", file.display()),
            Subject::File(path) => path.display().to_string(),
        }
    }

    /// Find this subject in an index built since it was chosen.
    fn resolve(&self, index: &Index) -> Option<SymbolId> {
        let Subject::Symbol { file, name, kind } = self else {
            return None;
        };
        index
            .symbols
            .iter()
            .find(|s| s.file == *file && s.name == *name && s.kind == *kind)
            .map(|s| s.id)
    }
}

/// What an operation did, and what it left alone.
type Outcome = (EditSet, Vec<String>);

/// The warnings a plan carried, flattened into lines a report can print.
fn said(warnings: &[crate::refactor::Warning]) -> Vec<String> {
    warnings
        .iter()
        .map(|w| format!("{}:{}:{} {}", w.file.display(), w.line, w.col, w.detail))
        .collect()
}

fn act(operation: &Operation, index: &Index, subject: &Subject, root: &Path) -> Result<Outcome> {
    if let Subject::File(path) = subject {
        return match operation {
            Operation::Imports => {
                let plan = crate::refactor::imports::plan(index, path)?;
                let warnings = said(&plan.warnings);
                Ok((plan.edits, warnings))
            }
            Operation::Rewrite { name } => Ok((rewrite_file(index, path, name)?, Vec::new())),
            other => bail!("`{}` acts on a symbol, not a file", other.describe()),
        };
    }
    let id = subject
        .resolve(index)
        .context("the symbol is no longer in the index")?;
    match operation {
        Operation::Rename { to } => {
            let plan = crate::refactor::rename::plan(index, id, to)?;
            let warnings = said(&plan.warnings);
            Ok((plan.edits, warnings))
        }
        Operation::Delete => {
            let plan = crate::refactor::delete::plan(index, id)?;
            let warnings = said(&plan.warnings);
            Ok((plan.edits, warnings))
        }
        Operation::Move { to } => {
            let plan = crate::refactor::move_symbol::to_file(index, id, &root.join(to))?;
            let warnings = plan.warnings.clone();
            Ok((plan.edits, warnings))
        }
        Operation::Inline { call: false } => Ok((
            crate::refactor::inline::variable(index, id)?.edits,
            Vec::new(),
        )),
        Operation::Inline { call: true } => {
            let symbol = index.symbol(id).context("the symbol went away")?;
            Ok((
                crate::refactor::inline::call(index, &symbol.file, symbol.name_span.start)?.edits,
                Vec::new(),
            ))
        }
        Operation::Signature { change } => {
            let change = crate::refactor::signature::Change::parse(change)?;
            Ok((
                crate::refactor::signature::change(index, id, change)?.edits,
                Vec::new(),
            ))
        }
        other => bail!("`{}` acts on a file, not a symbol", other.describe()),
    }
}

/// Apply a micro-rewrite everywhere in a file that it applies.
///
/// The most dangerous statement in the language: `guard-clause` was once wrong at
/// 1,258 of 1,498 sites in helm/helm. It is the one that most needs `limit`, the dry
/// run and an `expect`.
fn rewrite_file(index: &Index, path: &Path, name: &str) -> Result<EditSet> {
    use crate::refactor::rewrite::{self, Rewrite};
    let Some(rewrite) = Rewrite::from_name(name) else {
        let known: Vec<&str> = Rewrite::ALL.iter().map(|r| r.as_str()).collect();
        bail!("unknown rewrite '{name}'. Known: {}", known.join(", "));
    };
    let source = crate::vfs::read_to_string(path)?;

    // Offsets shift as edits land, so this collects the sites once against the file as
    // it is and applies them together; the engine rejects overlaps.
    let mut edits = EditSet::new();
    let mut applied = 0;
    for offset in 0..source.len() {
        if !source.is_char_boundary(offset) {
            continue;
        }
        let Ok(available) = rewrite::available(index, path, offset) else {
            continue;
        };
        if !available.contains(&rewrite) {
            continue;
        }
        if let Ok(plan) = rewrite::apply(index, path, offset, rewrite) {
            let overlaps = plan.edits.paths().any(|p| {
                plan.edits.edits_for(p).is_some_and(|new| {
                    edits.edits_for(p).is_some_and(|existing| {
                        new.iter()
                            .any(|n| existing.iter().any(|e| n.span.overlaps(e.span)))
                    })
                })
            });
            if !overlaps {
                merge(&mut edits, &plan.edits);
                applied += 1;
            }
        }
    }
    if applied == 0 {
        bail!("`{name}` applies nowhere in {}", path.display());
    }
    Ok(edits)
}

fn extract_at(index: &Index, root: &Path, at: &str, name: &str, function: bool) -> Result<EditSet> {
    let (relative, start, end) = crate::span::parse_range(at)?;
    let path = root.join(relative);
    let source = crate::vfs::read_to_string(&path)?;
    let lines = crate::span::LineIndex::new(&source);
    let span = crate::span::Span::new(
        lines
            .offset(start, &source)
            .context("the range starts past the end of the file")?,
        lines
            .offset(end, &source)
            .context("the range ends past the end of the file")?,
    );
    Ok(if function {
        crate::refactor::extract::function(index, &path, span, name)?.edits
    } else {
        crate::refactor::extract::variable(index, &path, span, name, false)?.edits
    })
}

// ------------------------------------------------------------------- selection

/// The predicates this build answers, for the error that lists them.
pub const PREDICATES: &[&str] = &[
    "name",
    "kind",
    "lang",
    "file",
    "in",
    "annotated-with",
    "exported",
    "unused",
    "duplicated",
    "changed",
];

fn select(
    step: &Step,
    index: &Index,
    changed: &BTreeSet<PathBuf>,
    options: &Options,
) -> Result<Vec<Subject>> {
    for predicate in &step.selector {
        if !PREDICATES.contains(&predicate.field()) {
            let closest = PREDICATES
                .iter()
                .min_by_key(|known| distance(known, predicate.field()));
            bail!(
                "line {}: there is no predicate called `{}`{}. This build answers: {}.",
                step.line,
                predicate.field(),
                match closest {
                    Some(name) if distance(name, predicate.field()) <= 3 =>
                        format!(" — did you mean `{name}`?"),
                    _ => String::new(),
                },
                PREDICATES.join(", ")
            );
        }
    }

    // `imports` and `rewrite` act on files; everything else acts on symbols.
    let by_file = matches!(
        step.operation,
        Operation::Imports | Operation::Rewrite { .. }
    );

    let unused: BTreeSet<SymbolId> = if wants(&step.selector, "unused") {
        let mut catalog = crate::analysis::entrypoints::Catalog::builtin()?;
        for dir in options.catalogs {
            catalog.load_dir(dir)?;
        }
        let entrypoints = Entrypoints::from_catalog(&catalog, index);
        crate::refactor::delete::find_unused(index, &entrypoints)
            .into_iter()
            .collect()
    } else {
        BTreeSet::new()
    };

    let duplicated: BTreeSet<PathBuf> = if wants(&step.selector, "duplicated") {
        crate::analysis::duplicates::find(index, &Default::default())
            .map(|classes| {
                classes
                    .iter()
                    .flat_map(|class| class.instances.iter().map(|c| c.file.clone()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };

    if by_file {
        let mut files: Vec<PathBuf> = index
            .files()
            .map(|(path, _)| path)
            .filter(|path| {
                step.selector
                    .iter()
                    .all(|p| file_matches(p, path, index, changed, &duplicated))
            })
            .cloned()
            .collect();
        files.sort();
        return Ok(files.into_iter().map(Subject::File).collect());
    }

    let mut chosen: Vec<(PathBuf, usize, Subject)> = index
        .symbols
        .iter()
        .filter(|symbol| {
            step.selector
                .iter()
                .all(|p| symbol_matches(p, symbol, changed, &unused, &duplicated))
        })
        .map(|s| {
            (
                s.file.clone(),
                s.name_span.start,
                Subject::Symbol {
                    file: s.file.clone(),
                    name: s.name.clone(),
                    kind: s.kind,
                },
            )
        })
        .collect();
    // Stable order, so a `limit` takes the same sites every run.
    chosen.sort();
    Ok(chosen.into_iter().map(|(_, _, subject)| subject).collect())
}

fn wants(selector: &[Predicate], field: &str) -> bool {
    selector.iter().any(|p| p.field() == field)
}

fn symbol_matches(
    predicate: &Predicate,
    symbol: &crate::model::Symbol,
    changed: &BTreeSet<PathBuf>,
    unused: &BTreeSet<SymbolId>,
    duplicated: &BTreeSet<PathBuf>,
) -> bool {
    let path = symbol.file.to_string_lossy().to_string();
    match predicate {
        Predicate::Equals { field, value } => match field.as_str() {
            "name" => symbol.name == *value,
            "kind" => kind_name(symbol.kind) == value,
            "lang" => symbol.language.name() == value,
            "in" => path.contains(value.trim_end_matches('/')),
            "file" => path.ends_with(value.as_str()),
            "annotated-with" => annotated(symbol, value),
            _ => false,
        },
        Predicate::Glob { field, pattern } => match field.as_str() {
            "name" => glob(pattern, &symbol.name),
            "file" => glob(pattern, &path),
            _ => false,
        },
        Predicate::Flag { field, expected } => {
            let holds = match field.as_str() {
                "exported" => symbol.exported,
                "unused" => unused.contains(&symbol.id),
                "changed" => changed.contains(&symbol.file),
                "duplicated" => duplicated.contains(&symbol.file),
                _ => false,
            };
            holds == *expected
        }
    }
}

fn file_matches(
    predicate: &Predicate,
    path: &Path,
    index: &Index,
    changed: &BTreeSet<PathBuf>,
    duplicated: &BTreeSet<PathBuf>,
) -> bool {
    let text = path.to_string_lossy().to_string();
    let language = index.file(path).map(|info| info.language);
    match predicate {
        Predicate::Equals { field, value } => match field.as_str() {
            "lang" => language.map(|l| l.name()) == Some(value.as_str()),
            "in" => text.contains(value.trim_end_matches('/')),
            "file" => text.ends_with(value.as_str()),
            _ => false,
        },
        Predicate::Glob { field, pattern } => match field.as_str() {
            "file" => glob(pattern, &text),
            _ => false,
        },
        Predicate::Flag { field, expected } => {
            let holds = match field.as_str() {
                "changed" => changed.contains(path),
                "duplicated" => duplicated.contains(path),
                _ => false,
            };
            holds == *expected
        }
    }
}

/// Is this annotated with `wanted`?
///
/// Answered by the entry-point catalogue's own matcher, so a recipe's
/// `annotated-with="test"` and a catalogue rule's mean the same thing by construction.
/// There is no field on `Symbol` to read: an annotation is written *above* a
/// definition and is recovered from the source.
fn annotated(symbol: &crate::model::Symbol, wanted: &str) -> bool {
    crate::analysis::entrypoints::annotated_with(symbol, wanted)
}

fn kind_name(kind: SymbolKind) -> &'static str {
    kind.as_str()
}

/// `*` matches any run of characters; everything else is literal.
fn glob(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return true;
    };
    if !text.starts_with(first) {
        return false;
    }
    let mut at = first.len();
    let rest: Vec<&str> = parts.collect();
    let last = rest.len().saturating_sub(1);
    for (i, part) in rest.iter().enumerate() {
        if part.is_empty() {
            if i == last {
                return true;
            }
            continue;
        }
        let Some(found) = text[at..].find(part) else {
            return false;
        };
        at += found + part.len();
    }
    if pattern.ends_with('*') {
        return true;
    }
    at == text.len()
}

/// Edit distance, for "did you mean".
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push(
                (previous[j + 1] + 1)
                    .min(current[j] + 1)
                    .min(previous[j] + cost),
            );
        }
        previous = current;
    }
    previous[b.len()]
}
