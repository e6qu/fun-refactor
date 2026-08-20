//! Running a recipe: select, act, re-index, and say what happened.
//!
//! A recipe is **one transaction**. Every step's edits are applied to an in-memory copy of the
//! workspace and the index is rebuilt from it. So each step sees what the previous one left;
//! nothing reaches disk until the whole run has succeeded. A half-applied recipe leaves a
//! repository in a state nobody designed, the flag removed and its dead branches still there.

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
    /// How many steps the recipe holds, whether or not the run reached them.
    ///
    /// A recipe is a reviewable artifact, and its length is a fact about the file. The
    /// header counted `steps` instead. So a stopped run of a three-step recipe called
    /// itself a two-step one, against the `--explain` of the same file.
    pub steps_in_recipe: usize,
    pub steps: Vec<StepReport>,
    pub expectations: Vec<ExpectReport>,
    /// Files whose text differs from where the run started.
    pub files_changed: usize,
    /// Did every expectation hold, and was every refusal permitted?
    pub ok: bool,
    /// Why the run ended early, when a refusal stopped it.
    ///
    /// This used to ride along as one more [`StepReport`], which made a stopped two-step
    /// recipe report three steps. A stop is a verdict on the run, so it is carried apart
    /// from the steps and the step count stays the count of steps.
    pub stopped: Option<String>,
    /// True when this run's edits reached the disk.
    ///
    /// The run itself only plans; the CLI sets this before printing. Without it an
    /// agent reading a failed `--write` report could not tell whether the disk
    /// holds the recipe's changes or the text it started from.
    pub applied: bool,
    /// True when `--write` was asked for and the workspace was left untouched
    /// because the run failed: a stop, or an expectation that did not hold.
    pub rolled_back: bool,
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
    /// A refusal is the operation declining. A warning is it succeeding and telling you what it
    /// could not verify: a reference that resolved too weakly to rewrite, a name in a comment.
    /// Dropping these on the floor is the accept-and-ignore this codebase bans elsewhere: `fr
    /// rename` prints them and a recipe was swallowing them. So a step that left work behind
    /// reported a clean run.
    ///
    /// The shape is the one `fr rename --json` emits for the same facts. They were
    /// flat prose here, and an agent reading both surfaces had to parse one of them.
    pub warnings: Vec<StepWarning>,
    pub files_changed: usize,
}

/// A warning in the shape the standalone commands emit: place, kind, prose.
#[derive(Debug, Serialize)]
pub struct StepWarning {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    pub kind: String,
    pub detail: String,
}

impl StepWarning {
    /// The one-line spelling the human report prints.
    pub fn describe(&self) -> String {
        format!(
            "{}:{}:{} {}",
            self.file.display(),
            self.line,
            self.col,
            self.detail
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Refusal {
    pub subject: String,
    pub reason: String,
    /// True when `on-refusal allow` said these were expected.
    pub permitted: bool,
    /// The positions the refusal is about, where the refusing operation named them.
    pub references: Vec<crate::refactor::RefusalSite>,
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

    // Files this run has already touched, which `where changed` selects on.
    let mut changed: BTreeSet<PathBuf> = BTreeSet::new();
    let mut steps = Vec::new();
    let mut total_refusals = 0usize;
    let mut stopped = None;

    // Only what an expectation asks for. Both analyses run over the whole
    // workspace, and both ran twice, before and after, even for a recipe with no
    // `expect no-new` in it. Over helm that is most of a minute spent answering a
    // question nobody asked.
    let wanted: BTreeSet<&str> = recipe
        .expects
        .iter()
        .filter_map(|e| match e {
            Expect::NoNew(what) => Some(what.as_str()),
            _ => None,
        })
        .collect();
    let before = analyses(&index, options, &wanted)?;

    for step in &recipe.steps {
        let report = run_step(step, &index, &sources, &changed, options)?;

        // `stop` is the default because a step that refused has not done what the recipe says
        // it does. The steps after it were written expecting it had.
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
        let after = analyses(&index, options, &wanted)?;
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
                steps_in_recipe: recipe.steps.len(),
                steps,
                expectations,
                files_changed: 0,
                ok: false,
                stopped: Some(why),
                applied: false,
                rolled_back: false,
            },
            // Nothing is written: the transaction did not complete.
            originals,
        ));
    }

    Ok((
        Report {
            recipe: recipe.name.clone(),
            description: recipe.description.clone(),
            steps_in_recipe: recipe.steps.len(),
            steps,
            expectations,
            files_changed,
            ok,
            stopped: None,
            applied: false,
            rolled_back: false,
        },
        sources,
    ))
}

/// Rebuild the index, and hand the same text to everything that reads a file.
///
/// The refactorings read source through [`crate::vfs`], not from this map. So without
/// installing it a plan made after one step is measured against the file on disk, which is the
/// text before *any* step ran.
fn reindex(sources: &Sources) -> Result<Index> {
    // Extraction is per-file and depends only on the file's bytes, and a step
    // touches a handful of them. Rebuilding every file's facts on every step
    // made a two-step recipe here take three cold index builds. Minutes went
    // to re-reading files no step had touched. The facts
    // cache is keyed by content, so an unchanged file costs a lookup.
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static FACTS: RefCell<HashMap<(PathBuf, u64), crate::model::FileFacts>> =
            RefCell::new(HashMap::new());
    }
    let handle = crate::vfs::new_handle(
        sources
            .iter()
            .map(|(path, (_, text))| (path.clone(), text.clone())),
    );
    crate::vfs::activate(&handle);
    let parsers = crate::parse::Parsers::new();
    let mut extractor = crate::extract::Extractor::new();
    let mut extracted = Vec::with_capacity(sources.len());
    for (path, (language, text)) in sources.iter() {
        let key = (path.clone(), crate::index::content_hash_of(text));
        let cached = FACTS.with(|facts| facts.borrow().get(&key).cloned());
        let facts = match cached {
            Some(facts) => facts,
            None => {
                let fresh =
                    crate::index::extract_facts(&parsers, &mut extractor, path, *language, text)?;
                FACTS.with(|facts| facts.borrow_mut().insert(key, fresh.clone()));
                fresh
            }
        };
        extracted.push((path.clone(), *language, facts));
    }
    let mut index = Index::build_from_facts(&extracted);
    for (path, (_, text)) in sources.iter() {
        index.note_content_hash(path.clone(), crate::index::content_hash_of(text));
    }
    Ok(index)
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
                    bail!("`requires language {name}`. No such language");
                };
                if !sources.values().any(|(l, _)| *l == language) {
                    bail!(
                        "`requires language {name}`. This workspace has no {name} file, so \
                         the recipe would do nothing and say it had succeeded"
                    );
                }
            }
            Requirement::Symbol(name) => {
                if !index.symbols.iter().any(|s| s.name == *name) {
                    bail!(
                        "`requires symbol \"{name}\"`. Nothing in this workspace is called \
                         that. The recipe was written for a different tree."
                    );
                }
            }
            Requirement::Path(path) => {
                if !sources.keys().any(|p| p.to_string_lossy().contains(path)) {
                    bail!("`requires path \"{path}\"`. No file in this workspace is under it");
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

fn analyses(index: &Index, options: &Options, wanted: &BTreeSet<&str>) -> Result<Analyses> {
    let unused = if wanted.contains("unused") {
        let mut catalog = crate::analysis::entrypoints::Catalog::builtin()?;
        for dir in options.catalogs {
            catalog.load_dir(dir)?;
        }
        let entrypoints = Entrypoints::from_catalog(&catalog, index);
        crate::refactor::delete::find_unused(index, &entrypoints)
            .into_iter()
            .filter_map(|id| index.symbol(id))
            .map(|s| format!("{}:{}", s.file.display(), s.name))
            .collect()
    } else {
        BTreeSet::new()
    };
    let duplicates = if wanted.contains("duplicates") {
        crate::analysis::duplicates::find(index, &Default::default())
            .map(|classes| classes.len())
            .unwrap_or(0)
    } else {
        0
    };
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
    let mut warnings: Vec<StepWarning> = Vec::new();
    let mut applied = 0usize;
    let permitted = step.on_refusal == OnRefusal::Allow;

    // The workspace-wide operations take no `where`: the signature table already
    // rejected one. `remove-flag` and the extracts name their one target directly,
    // while a `restructure` pattern selects its occurrences, so its count is real.
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
                    references: refusal_sites(&e),
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
                    // The pattern is this step's selector, so the counts are per
                    // occurrence. A constant 1 here reported "matched 1, applied 0"
                    // for a pattern that matched nothing. That read as a quiet
                    // success and slipped past the empty-match stop below.
                    applied = plan.matches.len();
                    merge(&mut edits, &plan.edits);
                    plan.matches.len()
                }
                Err(e) => {
                    refusals.push(Refusal {
                        subject: pattern.clone(),
                        reason: e.to_string(),
                        permitted,
                        references: refusal_sites(&e),
                    });
                    // A refusal is its own verdict; the count only has to keep the
                    // empty-match stop from speaking over it.
                    1
                }
            }
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
                    references: refusal_sites(&e),
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
            // Rebuilding the index after every one of them makes a step unusable at scale. Five
            // files of helm took two minutes because each subject re-indexed all five hundred
            // and thirty-nine.
            //
            // It is needed when a previous edit could have moved the text this subject is
            // about. That is true when its own file has already been edited. True for
            // everything once an operation has edited a file other than its subject's, a rename
            // rewrites call sites elsewhere. Otherwise the subjects are independent and one
            // index does for all of them.
            let mut running = sources.clone();
            let mut current = reindex(&running)?;
            let mut touched: BTreeSet<PathBuf> = BTreeSet::new();
            let mut reaches_other_files = false;
            for subject in taken {
                if matches!(subject, Subject::Symbol { .. }) && subject.resolve(&current).is_none()
                {
                    // Already gone, an earlier subject's cascade took it. That is the
                    // step succeeding, not failing.
                    continue;
                }
                let home = match &subject {
                    Subject::Symbol { file, .. } | Subject::File(file) => file.clone(),
                };
                if reaches_other_files || touched.contains(&home) {
                    current = reindex(&running)?;
                }
                match act(&step.operation, &current, &subject, options.root) {
                    Ok((set, said, sites)) => {
                        applied += sites;
                        warnings.extend(said);
                        for path in set.paths() {
                            if *path != home {
                                reaches_other_files = true;
                            }
                            touched.insert(path.clone());
                        }
                        apply(&mut running, &set)?;
                    }
                    Err(e) => refusals.push(Refusal {
                        subject: subject.describe(),
                        reason: e.to_string(),
                        permitted,
                        references: refusal_sites(&e),
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
    // failure this most wants to avoid, because it looks like success.
    if matched == 0 && !step.allow_empty {
        bail!(
            "line {}: `{}` matched nothing. That is not success. Write `allow-empty` if \
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
/// A symbol is named and not identified. Each subject is acted on against a
/// freshly built index, because one deletion moves every span after it in the file and
/// a `SymbolId` does not survive a rebuild, planning them all against one snapshot
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

/// What an operation did, what it left alone, and how many sites it touched.
///
/// The count is 1 for anything acting on a symbol. A `rewrite` counts however many
/// sites it found in one file, because the unit that matters there is the site. It is
/// not the file, which is what `limit` is for.
type Outcome = (EditSet, Vec<StepWarning>, usize);

/// The warnings a plan carried, in the shape the standalone commands emit them.
fn said(warnings: &[crate::refactor::Warning]) -> Vec<StepWarning> {
    warnings
        .iter()
        .map(|w| StepWarning {
            file: w.file.clone(),
            line: w.line,
            col: w.col,
            kind: w.kind.as_str().to_string(),
            detail: w.detail.clone(),
        })
        .collect()
}

/// The positions a refusal carried, empty when the error was not a refusal.
fn refusal_sites(error: &anyhow::Error) -> Vec<crate::refactor::RefusalSite> {
    crate::refactor::refusal_in(error)
        .map(|refusal| refusal.references().to_vec())
        .unwrap_or_default()
}

fn act(operation: &Operation, index: &Index, subject: &Subject, root: &Path) -> Result<Outcome> {
    if let Subject::File(path) = subject {
        return match operation {
            Operation::Imports => {
                let plan = crate::refactor::imports::plan(index, path)?;
                let warnings = said(&plan.warnings);
                Ok((plan.edits, warnings, 1))
            }
            Operation::Rewrite { name } => {
                let (edits, sites) = rewrite_file(index, path, name)?;
                Ok((edits, Vec::new(), sites))
            }
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
            Ok((plan.edits, warnings, 1))
        }
        Operation::Delete => {
            let plan = crate::refactor::delete::plan(index, id)?;
            let warnings = said(&plan.warnings);
            Ok((plan.edits, warnings, 1))
        }
        Operation::Move { to } => {
            let symbol = index.symbol(id).context("the symbol went away")?;
            let home = symbol.file.clone();
            let at = crate::vfs::read_to_string(&home)
                .map(|source| {
                    crate::span::LineIndex::new(&source).line_col(symbol.name_span.start, &source)
                })
                .unwrap_or(crate::span::LineCol { line: 1, col: 1 });
            let plan = crate::refactor::move_symbol::to_file(index, id, &root.join(to))?;
            // A move's warnings are prose about the whole move. Each is pinned to
            // the symbol being moved, the place a reviewer starts from.
            let warnings = plan
                .warnings
                .iter()
                .map(|detail| StepWarning {
                    file: home.clone(),
                    line: at.line,
                    col: at.col,
                    kind: "move-check".to_string(),
                    detail: detail.clone(),
                })
                .collect();
            Ok((plan.edits, warnings, 1))
        }
        Operation::Inline { call: false } => Ok((
            crate::refactor::inline::variable(index, id)?.edits,
            Vec::new(),
            1,
        )),
        Operation::Inline { call: true } => {
            let symbol = index.symbol(id).context("the symbol went away")?;
            Ok((
                crate::refactor::inline::call(index, &symbol.file, symbol.name_span.start)?.edits,
                Vec::new(),
                1,
            ))
        }
        Operation::Signature { change } => {
            let change = crate::refactor::signature::Change::parse(change)?;
            Ok((
                crate::refactor::signature::change(index, id, change)?.edits,
                Vec::new(),
                1,
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
fn rewrite_file(index: &Index, path: &Path, name: &str) -> Result<(EditSet, usize)> {
    use crate::refactor::rewrite::{self, Rewrite};
    let Some(rewrite) = Rewrite::from_name(name) else {
        let known: Vec<&str> = Rewrite::ALL.iter().map(|r| r.as_str()).collect();
        bail!("unknown rewrite '{name}'. Known: {}", known.join(", "));
    };
    let source = crate::vfs::read_to_string(path)?;
    let Some(language) = index.file(path).map(|info| info.language) else {
        bail!("{} is not in the index", path.display());
    };

    // Candidate positions come from one parse of the file. Asking at *every byte offset* worked
    // and took two minutes forty over five files of helm, because each ask reparses. It is
    // O(bytes × parse) where it wants to be O(anchors × parse). All three transformations
    // anchor on a conditional or on a negation, so those are the only offsets worth asking
    // about.
    let parsers = crate::parse::Parsers::new();
    let parsed = parsers.parse(language, &source)?;
    let mut anchors: Vec<usize> = Vec::new();
    let mut stack = vec![parsed.root()];
    let mut cursor = parsed.root().walk();
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind.contains("if") || kind.contains("unary") || kind.contains("not") {
            anchors.push(node.start_byte());
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    anchors.sort_unstable();
    anchors.dedup();

    // Offsets shift as edits land, so this collects the sites against the file as it
    // is and applies them together; the engine rejects overlaps.
    let mut edits = EditSet::new();
    let mut applied = 0;
    for offset in anchors {
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
    // A file where the transformation applies nowhere is not a refusal. The selector
    // chose *files*, and a file with no wrapping `if` in it had nothing to do.
    // Treating that as a failure made `on-refusal stop`, the default, abandon the run
    // on the first ordinary file. Over one package of helm that was three of five.
    Ok((edits, applied))
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
    "calls",
    "called-by",
    "implements",
    "matches",
];

/// The workspace-wide answers a selector needed, computed once per step.
///
/// Each is an existing analysis and not new machinery. Each is only run when a predicate asks
/// for it. The call graph over helm is not something to build for a selector that says
/// `name="x"`.
#[derive(Default)]
struct Facts {
    unused: BTreeSet<SymbolId>,
    duplicated: BTreeSet<PathBuf>,
    /// Symbols that call the named one, and symbols the named one calls.
    calls: BTreeSet<SymbolId>,
    called_by: BTreeSet<SymbolId>,
    /// Concrete answers to the named abstraction.
    implements: BTreeSet<SymbolId>,
    /// Spans where a structural pattern matched, by file.
    matched: Vec<(PathBuf, crate::span::Span)>,
}

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
                "line {}: there is no predicate called `{}`.{} This build answers: {}.",
                step.line,
                predicate.field(),
                match closest {
                    Some(name) if distance(name, predicate.field()) <= 3 =>
                        format!(" Did you mean `{name}`?"),
                    _ => String::new(),
                },
                PREDICATES.join(", ")
            );
        }
        // `kind` and `lang` take a value from a closed set, and a misspelled one used to be
        // answered by the codebase and not by the recipe: `kind=functoin` matched nothing, and
        // the step failed saying it had matched nothing, which blames the repository for a typo
        // in the file. The predicate's own name is checked above for this reason; its value
        // deserves the same.
        //
        // `kind` is checked by parsing it, so the vocabulary comes from the type and not
        // from a list kept beside it. Serde's own error names the alternatives.
        if let Predicate::Equals { field, value } = predicate {
            match field.as_str() {
                "kind" => {
                    if let Err(e) = serde_json::from_value::<SymbolKind>(serde_json::Value::String(
                        value.clone(),
                    )) {
                        bail!("line {}: {e}", step.line);
                    }
                }
                "lang" if Language::from_name(value).is_none() => {
                    let known: Vec<&str> = Language::ALL.iter().map(|l| l.name()).collect();
                    let closest = known.iter().min_by_key(|name| distance(name, value));
                    bail!(
                        "line {}: `{value}` is not a language.{} This build answers: {}.",
                        step.line,
                        match closest {
                            Some(name) if distance(name, value) <= 3 =>
                                format!(" Did you mean `{name}`?"),
                            _ => String::new(),
                        },
                        known.join(", ")
                    );
                }
                _ => {}
            }
        }
    }

    // `imports` and `rewrite` act on files; everything else acts on symbols.
    let by_file = matches!(
        step.operation,
        Operation::Imports | Operation::Rewrite { .. }
    );

    let facts = gather(step, index, options)?;

    if by_file {
        let mut files: Vec<PathBuf> = index
            .files()
            .map(|(path, _)| path)
            .filter(|path| {
                step.selector
                    .iter()
                    .all(|p| file_matches(p, path, index, changed, &facts))
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
                .all(|p| symbol_matches(p, symbol, changed, &facts))
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

/// The value a predicate was given, when it takes one.
fn argument<'a>(selector: &'a [Predicate], field: &str) -> Option<&'a str> {
    selector.iter().find_map(|p| match p {
        Predicate::Equals { field: f, value } if f == field => Some(value.as_str()),
        _ => None,
    })
}

/// Run the analyses this selector asks for, and only those.
fn gather(step: &Step, index: &Index, options: &Options) -> Result<Facts> {
    let mut facts = Facts::default();

    if wants(&step.selector, "unused") {
        let mut catalog = crate::analysis::entrypoints::Catalog::builtin()?;
        for dir in options.catalogs {
            catalog.load_dir(dir)?;
        }
        let entrypoints = Entrypoints::from_catalog(&catalog, index);
        facts.unused = crate::refactor::delete::find_unused(index, &entrypoints)
            .into_iter()
            .collect();
    }

    if wants(&step.selector, "duplicated") {
        facts.duplicated = crate::analysis::duplicates::find(index, &Default::default())
            .map(|classes| {
                classes
                    .iter()
                    .flat_map(|class| class.instances.iter().map(|c| c.file.clone()))
                    .collect()
            })
            .unwrap_or_default();
    }

    // One call graph answers both directions, and it is only built if one is asked for.
    if wants(&step.selector, "calls") || wants(&step.selector, "called-by") {
        let graph = crate::analysis::call_graph::CallGraph::build(index);
        if let Some(name) = argument(&step.selector, "calls") {
            // `calls="x"` selects the callers of x, the symbols with an edge into it.
            for target in named(index, name) {
                for (caller, _) in graph.callers(target) {
                    facts.calls.insert(caller);
                }
            }
        }
        if let Some(name) = argument(&step.selector, "called-by") {
            for caller in named(index, name) {
                for (callee, _) in graph.callees(caller) {
                    facts.called_by.insert(callee);
                }
            }
        }
    }

    if let Some(name) = argument(&step.selector, "implements") {
        let hierarchy = crate::analysis::call_graph::Hierarchy::scan(index);
        for abstraction in named(index, name) {
            for concrete in hierarchy.implementations_of(index, abstraction) {
                facts.implements.insert(concrete);
            }
        }
    }

    if let Some(pattern) = argument(&step.selector, "matches") {
        // A structural pattern is per-language, and the selector has to say which:
        // the same text is a different tree in every one of them.
        let Some(name) = argument(&step.selector, "lang") else {
            bail!(
                "line {}: `matches=` needs `lang=` beside it. The same text parses into a \
                 different tree in every language, so there is no language-free answer to \
                 where a shape occurs.",
                step.line
            );
        };
        let Some(language) = Language::from_name(name) else {
            bail!("line {}: no language called '{name}'", step.line);
        };
        facts.matched = crate::refactor::restructure::locate(index, language, pattern)?;
    }

    Ok(facts)
}

/// Every symbol with this name, since a selector names one instead of pointing at it.
fn named(index: &Index, name: &str) -> Vec<SymbolId> {
    index
        .symbols
        .iter()
        .filter(|s| s.name == name)
        .map(|s| s.id)
        .collect()
}

fn symbol_matches(
    predicate: &Predicate,
    symbol: &crate::model::Symbol,
    changed: &BTreeSet<PathBuf>,
    facts: &Facts,
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
            // These four are answered by an analysis and not by the symbol. So the value has
            // already been used to compute the set and only membership is left to check.
            "calls" => facts.calls.contains(&symbol.id),
            "called-by" => facts.called_by.contains(&symbol.id),
            "implements" => facts.implements.contains(&symbol.id),
            "matches" => facts
                .matched
                .iter()
                .any(|(file, span)| *file == symbol.file && symbol.full_span.contains(*span)),
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
                "unused" => facts.unused.contains(&symbol.id),
                "changed" => changed.contains(&symbol.file),
                "duplicated" => facts.duplicated.contains(&symbol.file),
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
    facts: &Facts,
) -> bool {
    let text = path.to_string_lossy().to_string();
    let language = index.file(path).map(|info| info.language);
    match predicate {
        Predicate::Equals { field, value } => match field.as_str() {
            "lang" => language.map(|l| l.name()) == Some(value.as_str()),
            "in" => text.contains(value.trim_end_matches('/')),
            "file" => text.ends_with(value.as_str()),
            "matches" => facts.matched.iter().any(|(file, _)| file == path),
            _ => false,
        },
        Predicate::Glob { field, pattern } => match field.as_str() {
            "file" => glob(pattern, &text),
            _ => false,
        },
        Predicate::Flag { field, expected } => {
            let holds = match field.as_str() {
                "changed" => changed.contains(path),
                "duplicated" => facts.duplicated.contains(path),
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
///
/// `pub(crate)` because the CLI's own "no symbol named" suggestion uses the same
/// measure; two distances would rank the same typo two ways.
pub(crate) fn distance(a: &str, b: &str) -> usize {
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
