//! Running a recipe: select, act, re-index, and say what happened.

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
    pub steps_in_recipe: usize,
    pub steps: Vec<StepReport>,
    pub expectations: Vec<ExpectReport>,
    /// Files whose text differs from where the run started.
    pub files_changed: usize,
    /// Did every expectation hold, and was every refusal permitted?
    pub ok: bool,
    /// Why the run ended early, when a refusal stopped it.
    pub stopped: Option<String>,
    /// True when this run's edits reached the disk.
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
    pub warnings: Vec<StepWarning>,
    pub files_changed: usize,
    /// Files this step wrote that the workspace did not have.
    pub files_created: Vec<PathBuf>,
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

    // Only what an expectation asks for.
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
        // it does.
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
fn reindex(sources: &Sources) -> Result<Index> {
    // Extraction is per-file and depends only on the file's bytes, and a step touches a handful
    // of them.
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
        // A step that creates a file says what it wrote, and the plan carries the answer.
        let entry = match sources.entry(path.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let language = edits
                    .language(path)
                    .or_else(|| crate::lang::detect(path))
                    .with_context(|| {
                        format!(
                            "{} would be created, and nothing says what language \
                             it is written in. The step declared none. The name does \
                             not say",
                            path.display()
                        )
                    })?;
                entry.insert((language, String::new()))
            }
        };
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

    // The workspace-wide operations take no `where`: the signature table already rejected one.
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
                    // The pattern is this step's selector, so the counts are per occurrence.
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
            let mut running = sources.clone();
            let mut current = reindex(&running)?;
            let mut touched: BTreeSet<PathBuf> = BTreeSet::new();
            let mut reaches_other_files = false;
            for subject in taken {
                if matches!(subject, Subject::Symbol { .. }) && subject.resolve(&current).is_none()
                {
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

            // The step's edit is the difference it made, whole-file: the individual spans were
            // measured against intermediate text that no longer exists.
            for (path, (language, text)) in &running {
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
                    edits.declare_language(path.clone(), *language);
                }
            }
            total
        }
    };

    // A selector that matches nothing stops the recipe.
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
            files_created: edits
                .paths()
                .filter(|path| !sources.contains_key(path.as_path()))
                .map(|path| {
                    path.strip_prefix(options.root)
                        .unwrap_or(path)
                        .to_path_buf()
                })
                .collect(),
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
        // What a plan says a file it creates is written in travels with the edits.
        if let Some(language) = from.language(path) {
            into.declare_language(path.clone(), language);
        }
    }
}

/// What a step acts on: a symbol, or a whole file.
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
            Operation::Translate { to } => {
                let (edits, warnings) = translate_file(path, to, root)?;
                Ok((edits, warnings, 1))
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
            // A move's warnings are prose about the whole move.
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
fn translate_file(path: &Path, to: &str, root: &Path) -> Result<(EditSet, Vec<StepWarning>)> {
    let Some(language) = crate::lang::Language::from_name(to) else {
        bail!("{to} is not a language this build knows");
    };
    let from = crate::lang::detect(path)
        .with_context(|| format!("{} is not a language this build knows", path.display()))?;
    if from == language {
        bail!("{} is already {language}", path.display());
    }

    // The standalone command offers `--force` and `--out` here, and a recipe can spell neither.
    let destination = crate::translate::destination_for(path, language)?;
    if crate::vfs::exists(&destination) {
        bail!(
            "{} is already there. A recipe writes a translation beside its source and \
             never over one: remove it, narrow the selector, or write `on-refusal \
             allow` to let the rest of the run proceed",
            destination
                .strip_prefix(root)
                .unwrap_or(&destination)
                .display()
        );
    }

    // Containment first: it is the same bytes and loses nothing, so where it
    // applies it is the better answer.
    if crate::translate::targets(from).contains(&language) {
        let plan = crate::translate::plan(path, language)?;
        return Ok((plan.edits, Vec::new()));
    }
    let plan = crate::transpile::plan(path, language)?;
    // A note is a loss the reader has to see, and most of them name the line of the source they
    // came from.
    let warnings = plan
        .fidelity
        .notes
        .iter()
        .map(|note| {
            let (line, detail) = match note.strip_prefix("line ") {
                Some(rest) => match rest.split_once(": ") {
                    Some((number, text)) => match number.parse::<usize>() {
                        Ok(line) => (line, text.to_string()),
                        Err(_) => (0, note.clone()),
                    },
                    None => (0, note.clone()),
                },
                None => (0, note.clone()),
            };
            StepWarning {
                file: relative_to(path, root),
                line,
                col: 0,
                kind: "translation-loss".to_string(),
                detail,
            }
        })
        .collect();
    Ok((plan.edits, warnings))
}

/// A path as the report should show it: under the workspace root where it is.
fn relative_to(path: &Path, root: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

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

    // Candidate positions come from one parse of the file.
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
    // A file where the transformation applies nowhere is not a refusal.
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

    // `imports`, `rewrite` and `translate` act on files; everything else acts on
    // symbols.
    let by_file = matches!(
        step.operation,
        Operation::Imports | Operation::Rewrite { .. } | Operation::Translate { .. }
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
        let hierarchy = crate::analysis::call_graph::Hierarchy::scanned(index);
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
            // These four are answered by an analysis and not by the symbol.
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
