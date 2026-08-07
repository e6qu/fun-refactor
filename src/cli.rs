//! Command-line surface.

use crate::analysis::call_graph::{CallGraph, Direction2};
use crate::analysis::entrypoints::Catalog;
use crate::index::Index;
use crate::lang::Language;
use crate::model::Symbol;
use crate::parse::Parsers;
use crate::scan::{scan, ScanOptions};
use crate::span::{LineCol, LineIndex};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "fr",
    version,
    about = "Multi-language refactoring and code intelligence",
    long_about = None
)]
struct Cli {
    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    /// Workspace root to operate on.
    #[arg(long, short = 'C', global = true, default_value = ".")]
    root: PathBuf,

    /// Re-read every file instead of reusing cached facts.
    #[arg(long, global = true)]
    no_cache: bool,

    #[command(subcommand)]
    command: Command,
}

/// Every subcommand's name, asked of the parser rather than written down again.
///
/// The site tells a reader what to type, and a command that was renamed leaves prose
/// that reads perfectly and does not run. A list maintained beside the parser would be
/// one more thing to keep in step; this one cannot disagree with it.
pub fn command_names() -> Vec<String> {
    use clap::CommandFactory;
    Cli::command()
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect()
}

#[derive(Subcommand)]
enum Command {
    /// Show what this tool can do, per language.
    Capabilities {
        /// Only this capability, e.g. rename, extract-variable.
        #[arg(long)]
        capability: Option<String>,
        /// Only this language.
        #[arg(long = "lang")]
        language: Option<String>,
        /// Emit the markdown table used in the README.
        #[arg(long)]
        markdown: bool,
    },
    /// Inspect or clear the fact cache.
    Cache {
        /// Delete every cached entry for the current query set.
        #[arg(long)]
        clear: bool,
    },
    /// List the source files fun-refactor can act on.
    Scan {
        /// Restrict to a language (repeatable), e.g. --lang rust --lang go.
        #[arg(long = "lang")]
        languages: Vec<String>,
    },
    /// Parse files and report syntax health.
    Parse {
        /// Restrict to a language (repeatable).
        #[arg(long = "lang")]
        languages: Vec<String>,
        /// Show per-language totals instead of per-file detail.
        #[arg(long)]
        stats: bool,
    },
    /// List defined symbols.
    Symbols {
        /// Restrict to a language (repeatable).
        #[arg(long = "lang")]
        languages: Vec<String>,
        /// Only symbols whose name contains this string.
        #[arg(long)]
        name: Option<String>,
        /// Only symbols of this kind, e.g. function, struct, key.
        #[arg(long)]
        kind: Option<String>,
        /// Show index-wide totals instead of listing symbols.
        #[arg(long)]
        stats: bool,
    },
    /// Show where a symbol is defined — every definition, not just one.
    ///
    /// A trait or interface method has as many definitions as implementations, and a
    /// CSS class is declared by every rule that names it.
    Def {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Show only the primary definition.
        #[arg(long)]
        first: bool,
    },
    /// Show a symbol's type: what the source declared, or what follows from what it did.
    ///
    /// The two are reported separately and never merged, because "the source said `int`"
    /// and "this holds an `int`" are different answers. A declared type is a contract. An
    /// inferred one is a derivation, shown with the evidence it was drawn from — a
    /// literal, a constructor call, the binding it was assigned from — so a reader can
    /// judge it. Where neither is available the answer is that there is no type written
    /// down, which is also different from both.
    Type {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
    },
    /// Show the concrete implementations of an abstract declaration.
    Implementations {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
    },
    /// Show every use of a symbol, grouped by file.
    Usages {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Include same-named occurrences that resolved elsewhere or not at all.
        #[arg(long)]
        include_unresolved: bool,
    },
    /// Show what calls a function.
    Callers {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// How many levels to walk.
        #[arg(long, default_value = "1")]
        depth: usize,
    },
    /// Show what a function calls.
    Callees {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// How many levels to walk.
        #[arg(long, default_value = "1")]
        depth: usize,
    },
    /// Extract an expression into a named binding, or statements into a function.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Extract {
        /// The region to extract, as `path:line:col-line:col`.
        range: String,
        /// Name for the new binding or function.
        name: String,
        /// Extract the selected statements into a function instead of a binding.
        #[arg(long)]
        function: bool,
        /// Replace every identical occurrence in the same block (bindings only).
        #[arg(long)]
        all: bool,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Replace a variable's uses with its value, or a call with the callee's body.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Inline {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Inline the call at that position rather than a variable.
        #[arg(long)]
        call: bool,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Change a function's parameters and update every call site.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Signature {
        /// Position as `path:line:col`, or a bare function name.
        target: String,
        /// `remove:<i>`, `move:<from>:<to>`, or `add:<i>:<declaration>:<argument>`.
        change: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Move a top-level symbol to another file, updating imports.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Move {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Destination file.
        destination: PathBuf,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Delete a symbol, refusing if anything still uses it.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Delete {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Find code that is written more than once.
    ///
    /// Compares structure rather than text, so a copy whose variables were renamed
    /// still matches — that is the copy a textual search will never find.
    Duplicates {
        /// Smallest duplicate to report, in tokens.
        #[arg(long, default_value_t = 60)]
        min_tokens: usize,
        /// Require identifiers and literals to match too, not only the structure.
        #[arg(long)]
        exact: bool,
        /// Only report duplicates in this language. Repeatable.
        #[arg(long = "language", value_name = "LANG")]
        languages: Vec<String>,
        /// Only report duplicates under this path prefix. Repeatable.
        #[arg(long = "path", value_name = "PREFIX")]
        paths: Vec<PathBuf>,
    },
    /// List symbols nothing appears to use.
    Unused {
        /// Additional catalog directory for entry-point rules.
        #[arg(long)]
        catalogs: Option<PathBuf>,
        /// Only report symbols in this language. Repeatable.
        ///
        /// Filters the report, not the index: reachability is still worked out
        /// across the whole workspace, so narrowing here cannot invent a dead
        /// symbol the way scanning a subdirectory would.
        #[arg(long = "language", value_name = "LANG")]
        languages: Vec<String>,
        /// Only report symbols under this path prefix. Repeatable.
        #[arg(long = "path", value_name = "PREFIX")]
        paths: Vec<PathBuf>,
        /// Only report symbols nothing outside their own file or package can see.
        ///
        /// An exported symbol with no use in the workspace may still be the public
        /// API of a library, which no amount of scanning this repository can rule
        /// out. Those are the ones this hides.
        #[arg(long)]
        internal: bool,
    },
    /// Remove unused imports and sort import blocks.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Imports {
        /// File to organize.
        file: PathBuf,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Derive an OpenAPI document from a Next.js route tree.
    ///
    /// The baseline a contract-preserving rewrite is checked against: build it before
    /// the rewrite, run the finished service, fetch its `/openapi.json`, and diff.
    /// Paths, methods and path parameters are exact; anything the source did not
    /// declare is listed rather than invented.
    Openapi {
        /// Write the document here instead of to standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write YAML instead of JSON.
        ///
        /// The same document either way. YAML is what a contract kept beside the code
        /// is usually written in, and it is what a person reads.
        #[arg(long)]
        yaml: bool,
    },
    /// Run a refactoring recipe: a file that says what to find, what to do to it,
    /// and what must be true afterwards.
    ///
    /// Prints a report and a diff by default; pass --write to apply it. A recipe is
    /// one transaction — either every step's edits are written or none are.
    Recipe {
        /// The recipe file.
        file: PathBuf,
        /// Apply the changes instead of printing a diff.
        #[arg(long)]
        write: bool,
        /// Additional catalog directory for entry-point rules, as `fr unused` takes.
        #[arg(long)]
        catalogs: Vec<PathBuf>,
    },
    /// Rewrite a file as another language, beside the original.
    ///
    /// Only where one grammar contains the other — CSS as SCSS, a manifest as a Helm
    /// template, TypeScript as TSX — and only when the file parses cleanly as the
    /// target. Omit the language to list what this file could be. Rewriting one
    /// programming language as another is a translation, not a refactoring, and is
    /// refused with the reason.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Translate {
        /// File to rewrite.
        file: PathBuf,
        /// Target language, or `fastapi` for a Next.js API route.
        language: Option<String>,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Remove a feature flag and everything that only existed to serve it.
    ///
    /// Prints a diff by default; pass --write to apply it.
    RemoveFlag {
        /// The flag's name.
        flag: String,
        /// The value to assume it always had.
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        value: bool,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Apply a local transformation, or list the ones that apply.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Rewrite {
        /// Position as `path:line:col`.
        target: String,
        /// Which transformation: invert-if, de-morgan, guard-clause.
        /// Omit to list what applies at that position.
        rewrite: Option<String>,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Rewrite every occurrence of a code shape.
    ///
    /// `$NAME` in the pattern matches any node and substitutes back into the
    /// template. Prints a diff by default; pass --write to apply it.
    Restructure {
        /// The shape to match, e.g. 'old_api($X)'.
        pattern: String,
        /// What to replace it with, e.g. 'new_api($X, None)'.
        template: String,
        /// Language to rewrite.
        #[arg(long = "lang")]
        language: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Trace where a value comes from or goes to.
    ///
    /// For Helm charts, `-f` and `--set` describe the invocation the answer is
    /// for: without them a values key that the command line could override is
    /// reported undecided, with them the same precedence order decides it.
    Flow {
        /// Direction: `back` (where does it come from) or `fwd` (where is it used).
        #[arg(value_parser = ["back", "fwd"])]
        direction: String,
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// How many hops to follow.
        #[arg(long, default_value = "5")]
        depth: usize,
        /// A values file passed to helm with -f (repeatable; later files win).
        #[arg(long = "values", short = 'f', value_name = "FILE")]
        values: Vec<PathBuf>,
        /// A helm --set assignment: a.b=c, a[0].b=c, or several comma-separated.
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Like --set, but helm keeps the value a string.
        #[arg(long = "set-string", value_name = "KEY=VALUE")]
        set_string: Vec<String>,
    },
    /// Trace configuration values into the code that reads them.
    Stitch {
        /// Only chains for this environment variable.
        #[arg(long)]
        env: Option<String>,
        /// Only variables nothing in the workspace reads.
        #[arg(long)]
        orphaned: bool,
    },
    /// Show everything a change to a symbol could affect.
    Impact {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// How far to follow call edges backwards (0 disables the call walk).
        #[arg(long, default_value = "3")]
        caller_depth: usize,
    },
    /// Export the call graph.
    Graph {
        /// Emit Graphviz DOT.
        #[arg(long)]
        dot: bool,
    },
    /// List detected entry points.
    Entrypoints {
        /// Only this kind, e.g. cli-main, http-route, test, infra-input.
        #[arg(long)]
        kind: Option<String>,
        /// Additional catalog directory to load.
        #[arg(long)]
        catalogs: Option<PathBuf>,
        /// Report functions not reachable from any entry point.
        #[arg(long)]
        unreachable: bool,
    },
    /// Rename a symbol and every reference that provably points at it.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Rename {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// The new name.
        new_name: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// List references to a symbol.
    Refs {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Include weakly-resolved references that share the name.
        #[arg(long)]
        include_unresolved: bool,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    match &cli.command {
        Command::Capabilities {
            capability,
            language,
            markdown,
        } => cmd_capabilities(&cli, capability.as_deref(), language.as_deref(), *markdown),
        Command::Cache { clear } => cmd_cache(&cli, *clear),
        Command::Scan { languages } => cmd_scan(&cli, languages),
        Command::Parse { languages, stats } => cmd_parse(&cli, languages, *stats),
        Command::Symbols {
            languages,
            name,
            kind,
            stats,
        } => cmd_symbols(&cli, languages, name.as_deref(), kind.as_deref(), *stats),
        Command::Def { target, first } => cmd_def(&cli, target, *first),
        Command::Type { target } => cmd_type(&cli, target),
        Command::Implementations { target } => cmd_implementations(&cli, target),
        Command::Usages {
            target,
            include_unresolved,
        } => cmd_usages(&cli, target, *include_unresolved),
        Command::Refs {
            target,
            include_unresolved,
        } => cmd_refs(&cli, target, *include_unresolved),
        Command::Rename {
            target,
            new_name,
            write,
        } => cmd_rename(&cli, target, new_name, *write),
        Command::Callers { target, depth } => cmd_trace(&cli, target, *depth, Direction2::Callers),
        Command::Callees { target, depth } => cmd_trace(&cli, target, *depth, Direction2::Callees),
        Command::Flow {
            direction,
            target,
            depth,
            values,
            set,
            set_string,
        } => {
            let inputs = crate::analysis::provenance::ValuesInputs::parse(values, set, set_string)?;
            cmd_flow(&cli, direction, target, *depth, &inputs)
        }
        Command::Extract {
            range,
            name,
            function,
            all,
            write,
        } => cmd_extract(&cli, range, name, *function, *all, *write),
        Command::Inline {
            target,
            call,
            write,
        } => cmd_inline(&cli, target, *call, *write),
        Command::Openapi { out, yaml } => cmd_openapi(&cli, out.as_deref(), *yaml),
        Command::Recipe {
            file,
            write,
            catalogs,
        } => cmd_recipe(&cli, file, *write, catalogs),
        Command::RemoveFlag { flag, value, write } => cmd_remove_flag(&cli, flag, *value, *write),
        Command::Rewrite {
            target,
            rewrite,
            write,
        } => cmd_rewrite(&cli, target, rewrite.as_deref(), *write),
        Command::Restructure {
            pattern,
            template,
            language,
            write,
        } => cmd_restructure(&cli, pattern, template, language, *write),
        Command::Delete { target, write } => cmd_delete(&cli, target, *write),
        Command::Duplicates {
            min_tokens,
            exact,
            languages,
            paths,
        } => cmd_duplicates(&cli, *min_tokens, *exact, languages, paths),
        Command::Unused {
            catalogs,
            languages,
            paths,
            internal,
        } => cmd_unused(&cli, catalogs.as_deref(), languages, paths, *internal),
        Command::Imports { file, write } => cmd_imports(&cli, file, *write),
        Command::Translate {
            file,
            language,
            write,
        } => cmd_translate(&cli, file, language.as_deref(), *write),
        Command::Move {
            target,
            destination,
            write,
        } => cmd_move(&cli, target, destination, *write),
        Command::Signature {
            target,
            change,
            write,
        } => cmd_signature(&cli, target, change, *write),
        Command::Stitch { env, orphaned } => cmd_stitch(&cli, env.as_deref(), *orphaned),
        Command::Impact {
            target,
            caller_depth,
        } => cmd_impact(&cli, target, *caller_depth),
        Command::Graph { dot } => cmd_graph(&cli, *dot),
        Command::Entrypoints {
            kind,
            catalogs,
            unreachable,
        } => cmd_entrypoints(&cli, kind.as_deref(), catalogs.as_deref(), *unreachable),
    }
}

fn cmd_trace(cli: &Cli, target: &str, depth: usize, direction: Direction2) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    if !symbol.kind.is_callable() {
        anyhow::bail!(
            "'{}' is a {}, not a function or method — it has no call edges",
            symbol.name,
            symbol.kind.as_str()
        );
    }

    let graph = CallGraph::build(&index);
    let trace = graph.trace(symbol.id, direction, depth);

    if cli.json {
        let nodes: Vec<_> = trace
            .nodes
            .iter()
            .map(|n| {
                let s = index.symbol(n.symbol);
                serde_json::json!({
                    "name": s.map(|s| s.qualified_name()),
                    "file": s.map(|s| s.file.clone()),
                    "depth": n.depth,
                    "confidence": n.caller.map(|(_, c)| c.as_str()),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "start": symbol.qualified_name(),
                "direction": match direction {
                    Direction2::Callers => "callers",
                    Direction2::Callees => "callees",
                },
                "nodes": nodes,
                "cycles": trace.cycles.len(),
            }))?
        );
        return Ok(());
    }

    print!("{}", trace.format_tree(&index));

    // Calls we could see but not resolve belong in the answer, not hidden.
    let related: Vec<_> = graph
        .unresolved
        .iter()
        .filter(|u| u.caller == Some(symbol.id))
        .collect();
    if direction == Direction2::Callees && !related.is_empty() {
        println!("\n{} unresolved call(s) from this function:", related.len());
        for u in related.iter().take(10) {
            println!("  {} [{}]", u.callee_name, u.confidence.as_str());
        }
        if related.len() > 10 {
            println!("  … and {} more", related.len() - 10);
        }
    }
    Ok(())
}

/// Render a plan's diff, report what it did, and optionally commit it.
fn present(cli: &Cli, edits: &crate::edit::EditSet, summary: &str, write: bool) -> Result<()> {
    let outcomes = crate::edit::plan(edits, crate::edit::Validation::ReparseStrict)?;

    if cli.json {
        let changes: Vec<_> = outcomes
            .iter()
            .map(|o| serde_json::json!({ "path": o.path, "diff": o.unified_diff() }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "summary": summary,
                "files_changed": outcomes.len(),
                "applied": write,
                "changes": changes,
            }))?
        );
        if write {
            crate::edit::commit(&outcomes)?;
        }
        return Ok(());
    }

    for outcome in &outcomes {
        print!("{}", outcome.unified_diff());
    }
    println!("\n{summary}");
    if write {
        let count = crate::edit::commit(&outcomes)?;
        println!("Applied to {count} file(s).");
    } else {
        println!("\nNothing written. Re-run with --write to apply.");
    }
    Ok(())
}

fn cmd_extract(
    cli: &Cli,
    range: &str,
    name: &str,
    as_function: bool,
    all: bool,
    write: bool,
) -> Result<()> {
    let (path, start, end) = crate::span::parse_range(range)?;
    let path = workspace_path(cli, &path)?;
    let source =
        crate::vfs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let index_of_lines = LineIndex::new(&source);
    let span = crate::span::Span::new(
        index_of_lines
            .offset(start, &source)
            .ok_or_else(|| anyhow::anyhow!("{start} is outside {}", path.display()))?,
        index_of_lines
            .offset(end, &source)
            .ok_or_else(|| anyhow::anyhow!("{end} is outside {}", path.display()))?,
    );

    let index = build_index(cli, &[])?;

    if as_function {
        let plan = crate::refactor::extract::function(&index, &path, span, name)?;
        let params: Vec<&str> = plan.parameters.iter().map(|p| p.name.as_str()).collect();
        let summary = format!(
            "extracted {} statement(s) into {}({}){}",
            plan.body.lines().filter(|l| !l.trim().is_empty()).count(),
            plan.name,
            params.join(", "),
            if plan.returns.is_empty() {
                String::new()
            } else {
                format!(" returning {}", plan.returns.join(", "))
            }
        );
        return present(cli, &plan.edits, &summary, write);
    }

    let plan = crate::refactor::extract::variable(&index, &path, span, name, all)?;
    let summary = format!(
        "extracted `{}` into {} ({} occurrence(s) replaced)",
        plan.expression.trim(),
        plan.name,
        plan.occurrences
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_inline(cli: &Cli, target: &str, as_call: bool, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;

    if as_call {
        // A call has no symbol of its own, so this form needs a position.
        let pos = parse_position(target).ok_or_else(|| {
            anyhow::anyhow!("inlining a call needs a position: path:line:col of the call")
        })?;
        let path = workspace_path(cli, &pos.path)?;
        let source = crate::vfs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let offset = LineIndex::new(&source)
            .offset(
                LineCol {
                    line: pos.line,
                    col: pos.col,
                },
                &source,
            )
            .with_context(|| format!("{}:{} is outside {}", pos.line, pos.col, path.display()))?;

        let plan = crate::refactor::inline::call(&index, &path, offset)?;
        let summary = format!(
            "inlined the call to {} as `{}`",
            plan.function, plan.expansion
        );
        return present(cli, &plan.edits, &summary, write);
    }

    let symbol = resolve_target(cli, &index, target)?;
    let plan = crate::refactor::inline::variable(&index, symbol.id)?;
    let summary = format!(
        "inlined `{}` into {} use site(s)",
        plan.name, plan.use_sites
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_signature(cli: &Cli, target: &str, change_spec: &str, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let change = crate::refactor::signature::Change::parse(change_spec)?;
    let plan = crate::refactor::signature::change(&index, symbol.id, change)?;
    let summary = crate::refactor::signature::describe(&index, &plan);
    present(cli, &plan.edits, &summary, write)
}

/// The destination file, spelled the way the index spells its paths.
///
/// A move works out an import path by comparing the destination against the file
/// that needs the import, so the two have to be written the same way. They are not
/// by default: the destination is whatever the caller typed, while indexed paths are
/// canonical, and on macOS `/var` and `/private/var` name the same directory. Left
/// alone that produced imports like `'../../../../../../../var/folders/…'`.
///
/// The file itself need not exist yet — a move usually creates it — so it is the
/// parent directory that is resolved, and a missing one is an error rather than a
/// path passed through untouched.
fn resolve_destination(cli: &Cli, destination: &std::path::Path) -> Result<std::path::PathBuf> {
    let absolute = if destination.is_absolute() {
        destination.to_path_buf()
    } else {
        cli.root.join(destination)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", absolute.display()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} does not name a file", absolute.display()))?;
    let parent = parent.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "cannot resolve the destination directory {}: {e}. Create it first, or \
             give a path inside an existing directory",
            parent.display()
        )
    })?;
    Ok(parent.join(name))
}

fn cmd_move(cli: &Cli, target: &str, destination: &std::path::Path, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let dest = resolve_destination(cli, destination)?;
    let plan = crate::refactor::move_symbol::to_file(&index, symbol.id, &dest)?;

    // A CSS move's entire safety story is a warning, so these cannot stay hidden.
    if !plan.warnings.is_empty() && !cli.json {
        println!("Check these before committing:");
        for warning in &plan.warnings {
            println!("  {warning}");
        }
        println!();
    }

    let summary = format!(
        "moved {} from {} to {} ({} file(s) gained an import)",
        plan.symbol,
        plan.from.display(),
        plan.to.display(),
        plan.imports_added.len()
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_delete(cli: &Cli, target: &str, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let plan = crate::refactor::delete::plan(&index, symbol.id)?;

    if !plan.warnings.is_empty() && !cli.json {
        println!("Review these before committing:");
        for w in plan.warnings.iter().take(20) {
            println!("  {}:{}:{}  {}", w.file.display(), w.line, w.col, w.detail);
        }
        if plan.warnings.len() > 20 {
            println!("  … and {} more", plan.warnings.len() - 20);
        }
        println!();
    }

    let summary = format!("deleted {} ({} definition site(s))", plan.name, plan.sites);
    present(cli, &plan.edits, &summary, write)
}

/// A path the caller typed, spelled the way the index spells its paths.
///
/// Relative paths resolve against the workspace root, not the shell's working
/// directory: `-C` says which workspace to operate on, and `fr -C ../helm refs
/// pkg/x.go:3:6` means that file in that workspace. Canonical, because the index is,
/// and a path that does not exist is an error — resolving it to itself and letting
/// the file read fail two frames later says "reading pkg/x.go: No such file", which
/// is true and unhelpful.
fn workspace_path(cli: &Cli, path: &std::path::Path) -> Result<PathBuf> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    joined.canonicalize().map_err(|e| {
        if path.is_absolute() || root == std::path::Path::new(".") {
            anyhow::anyhow!("{}: {e}", path.display())
        } else {
            anyhow::anyhow!(
                "{} does not exist in {} — paths are read relative to the workspace \
                 root, which -C set to that. ({e})",
                path.display(),
                root.display()
            )
        }
    })
}

/// Language names from the command line, refused rather than ignored.
///
/// A typo that silently narrowed a report to nothing would read as "nothing found",
/// which is the wrong answer given confidently.
fn parse_languages(names: &[String]) -> Result<Vec<crate::lang::Language>> {
    names
        .iter()
        .map(|name| {
            crate::lang::Language::from_name(name).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown language '{name}'. Known: {}",
                    crate::lang::Language::ALL
                        .iter()
                        .map(|l| l.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
        })
        .collect()
}

/// Path filters, spelled the way the index spells its paths.
///
/// Resolved against the workspace root rather than the shell's cwd, and canonical,
/// because the index holds canonical paths. The default root is `.`, so a filter
/// built from it reads `./pkg/action` and matches no absolute path at all — the
/// report then comes back empty and looks like a clean bill of health.
fn absolute_paths(cli: &Cli, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    paths
        .iter()
        .map(|p| {
            let joined = if p.is_absolute() {
                p.clone()
            } else {
                root.join(p)
            };
            joined
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("cannot resolve --path {}: {e}", joined.display()))
        })
        .collect()
}

fn cmd_duplicates(
    cli: &Cli,
    min_tokens: usize,
    exact: bool,
    languages: &[String],
    paths: &[PathBuf],
) -> Result<()> {
    use crate::analysis::duplicates;

    let index = build_index(cli, &[])?;
    let options = duplicates::Options {
        min_tokens,
        exact,
        languages: parse_languages(languages)?,
        paths: absolute_paths(cli, paths)?,
    };
    let classes = duplicates::find(&index, &options)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&classes)?);
        return Ok(());
    }

    if classes.is_empty() {
        println!(
            "No duplication of {min_tokens} tokens or more{}.",
            if exact { " (exact)" } else { "" }
        );
    }
    for class in &classes {
        println!(
            "{} copies, {} tokens each ({} redundant) — {}",
            class.instances.len(),
            class.tokens,
            class.redundant_tokens(),
            class.language
        );
        for instance in &class.instances {
            println!(
                "  {}:{}-{}",
                instance.file.display(),
                instance.start_line,
                instance.end_line
            );
        }
    }

    if !classes.is_empty() {
        let redundant: usize = classes.iter().map(|c| c.redundant_tokens()).sum();
        // The threshold belongs here as much as it does in the empty case. Finding
        // nothing "of 60 tokens or more" says what was looked for; finding three and
        // saying only "3 duplicated block(s)" reads as all of them.
        println!(
            "\n{} duplicated block(s) of {min_tokens} tokens or more, {redundant} \
             redundant token(s)",
            classes.len()
        );
        println!(
            "Structure is compared, not text, so a copy with renamed variables still \n\
             matches; pass --exact to require the names too. Only the largest block of \n\
             each duplication is listed — the statements inside it are duplicated as \n\
             well, and saying so again would bury the finding. Smaller copies exist in \n\
             most codebases and are not counted here; --min-tokens decides where the \n\
             line is."
        );
    }

    let skipped = duplicates::unparsed(&index, &options);
    if !skipped.is_empty() {
        println!(
            "\n{} file(s) were skipped because they do not parse, so duplication in \n\
             them is not reported:",
            skipped.len()
        );
        for path in skipped.iter().take(10) {
            println!("  {}", path.display());
        }
        if skipped.len() > 10 {
            println!("  … and {} more", skipped.len() - 10);
        }
    }
    Ok(())
}

fn cmd_unused(
    cli: &Cli,
    extra_catalogs: Option<&std::path::Path>,
    languages: &[String],
    paths: &[PathBuf],
    internal_only: bool,
) -> Result<()> {
    let index = build_index(cli, &[])?;
    let mut catalog = Catalog::builtin()?;
    if let Some(dir) = extra_catalogs {
        catalog.load_dir(dir)?;
    }
    let entrypoints = crate::analysis::entrypoints::Entrypoints::from_catalog(&catalog, &index);
    let unused = crate::refactor::delete::find_unused(&index, &entrypoints);

    let wanted = parse_languages(languages)?;
    let roots = absolute_paths(cli, paths)?;
    let keep = |s: &crate::model::Symbol| {
        (wanted.is_empty() || wanted.contains(&s.language))
            && (roots.is_empty() || roots.iter().any(|r| s.file.starts_with(r)))
            && (!internal_only || !s.exported)
    };
    let total = unused.len();
    let unused: Vec<_> = unused
        .into_iter()
        .filter(|id| index.symbol(*id).is_some_and(keep))
        .collect();

    if cli.json {
        let payload: Vec<_> = unused
            .iter()
            .filter_map(|id| index.symbol(*id))
            .map(|s| {
                serde_json::json!({
                    "name": s.qualified_name(),
                    "kind": s.kind.as_str(),
                    "file": s.file,
                    "language": s.language.name(),
                    "exported": s.exported,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let mut exported_count = 0usize;
    for symbol in unused.iter().filter_map(|id| index.symbol(*id)) {
        if symbol.exported {
            exported_count += 1;
        }
        println!(
            "{:<12} {:<34} {:<9} {}",
            symbol.kind.as_str(),
            symbol.qualified_name(),
            if symbol.exported { "exported" } else { "" },
            symbol.file.display()
        );
    }
    if unused.len() == total {
        println!("\n{} symbol(s) with no detected use", unused.len());
    } else {
        println!(
            "\n{} symbol(s) with no detected use, of {total} found across the workspace",
            unused.len()
        );
    }
    println!(
        "Reachability follows resolved call edges plus class-hierarchy dispatch \n\
         candidates, so a method reached only through a trait object, an interface \n\
         value or a base class is no longer listed. A function held in a map or a \n\
         struct field and called through it, and a name assembled at runtime, still \n\
         can be. Symbols whose name is spelled in any string literal are deliberately \n\
         left off, as are names beginning with an underscore, which say the author \n\
         meant them to go unused."
    );
    if exported_count > 0 && !internal_only {
        println!(
            "\n{exported_count} of these are exported. In a library that is the public \n\
             API, which nothing in this repository can be expected to call — pass \n\
             --internal to list only what is definitely dead here."
        );
    }
    Ok(())
}

fn cmd_imports(cli: &Cli, file: &std::path::Path, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let path = workspace_path(cli, file)?;
    let plan = crate::refactor::imports::plan(&index, &path)?;

    if !plan.removed.is_empty() && !cli.json {
        println!("Removing {} unused import(s):", plan.removed.len());
        for removed in &plan.removed {
            println!(
                "  line {}: {} (binds {})",
                removed.line,
                removed.path,
                removed.bindings.join(", ")
            );
        }
        println!(
            "\nLiveness is decided by name, so an import kept only for a trait, a \n\
             registration side effect or a doc comment would look unused. Check these.\n"
        );
    }

    let summary = format!(
        "{}: removed {} import(s), reordered {} block(s)",
        plan.file.display(),
        plan.removed.len(),
        plan.sorted_blocks
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_translate(
    cli: &Cli,
    file: &std::path::Path,
    language: Option<&str>,
    write: bool,
) -> Result<()> {
    let path = workspace_path(cli, file)?;
    let from = crate::lang::detect(&path)
        .ok_or_else(|| anyhow::anyhow!("{} is not a language this build reads", path.display()))?;

    let Some(language) = language else {
        // No target named: say what this file could be, and stop.
        let mut targets: Vec<crate::lang::Language> = crate::translate::targets(from).to_vec();
        // Plus every language this can be translated into, which is a different and
        // much weaker promise — a draft rather than the same bytes.
        if crate::transpile::supports(from) {
            for language in crate::transpile::SUPPORTED {
                if *language != from && !targets.contains(language) {
                    targets.push(*language);
                }
            }
        }
        if targets.is_empty() {
            println!(
                "{} is {from}, and there is no language it can be rewritten as.\n\n{}",
                path.display(),
                crate::translate::why_nothing(from)
            );
            return Ok(());
        }
        println!("{} is {from}. It could be written as:", path.display());
        if crate::transpile::nextjs::is_api_route(&path) {
            match crate::transpile::nextjs::plan(&path) {
                Ok(plan) => println!(
                    "  {:<10} -> {} (route {}, {})",
                    "fastapi",
                    plan.destination.display(),
                    plan.route,
                    plan.methods.join(", ")
                ),
                Err(e) => println!("  {:<10} not this file: {e}", "fastapi"),
            }
        }

        for target in &targets {
            let containment = crate::translate::targets(from).contains(target);
            let outcome = if containment {
                crate::translate::plan(&path, *target).map(|p| (p.destination, None))
            } else {
                crate::transpile::plan(&path, *target).map(|p| (p.destination, Some(p.fidelity)))
            };
            match outcome {
                Ok((destination, None)) => {
                    println!("  {target:<10} -> {} (same bytes)", destination.display())
                }
                Ok((destination, Some(f))) => println!(
                    "  {target:<10} -> {} (a draft: {}/{} signatures complete, {} \
                     construct(s) carried over)",
                    destination.display(),
                    f.signatures_complete,
                    f.functions,
                    f.carried_verbatim
                ),
                Err(e) => println!("  {target:<10} not this file: {e}"),
            }
        }
        return Ok(());
    };

    // `fastapi` is a framework rather than a language, and the translation into it
    // reads the file's *path* as well as its text — a Next.js route's URL is where it
    // sits on disk. It is therefore its own target rather than a flavour of Python.
    if language.eq_ignore_ascii_case("fastapi") {
        return cmd_translate_fastapi(cli, &path, write);
    }

    let to = crate::lang::Language::from_name(language)
        .ok_or_else(|| anyhow::anyhow!("unknown language '{language}'"))?;

    // Containment first — CSS as SCSS is the same bytes and needs no translation. A
    // pair that is not a containment is a translation, which is a different promise.
    if crate::translate::targets(from).contains(&to) {
        let plan = crate::translate::plan(&path, to)?;
        let summary = format!(
            "{} written as {} ({} -> {})",
            plan.source.display(),
            plan.destination.display(),
            plan.from,
            plan.to
        );
        return present(cli, &plan.edits, &summary, write);
    }

    let plan = crate::transpile::plan(&path, to)?;
    if !cli.json {
        let f = &plan.fidelity;
        println!(
            "{} -> {} ({} function(s), {} record(s), {} constant(s))",
            plan.from, plan.to, f.functions, f.records, f.constants
        );
        println!(
            "  signatures: {} complete, {} mentioning a type this tool does not know",
            f.signatures_complete, f.signatures_with_foreign_types
        );
        // Not every note is about a carried construct. A type the source never wrote
        // down, a name the target reserves, a base class a language without
        // inheritance cannot keep — those were computed honestly and then printed only
        // when something *else* had gone wrong, so a translation that lost a supertype
        // and nothing else reported a clean bill.
        if f.carried_verbatim > 0 {
            println!(
                "  {} construct(s) had no counterpart and are in the output as comments:",
                f.carried_verbatim
            );
        } else if !f.notes.is_empty() {
            println!("  {} thing(s) the output cannot say:", f.notes.len());
        }
        for note in f.notes.iter().take(10) {
            println!("    {note}");
        }
        if f.notes.len() > 10 {
            println!("    and {} more", f.notes.len() - 10);
        }
        if f.carried_verbatim > 0 {
            println!(
                "\n  This is a draft. It carries the signatures; the bodies it could not \n  \
                 translate are beside the code that replaces them."
            );
        }
        println!();
    }
    let summary = format!(
        "{} translated to {} ({} -> {})",
        plan.source.display(),
        plan.destination.display(),
        plan.from,
        plan.to
    );
    present(cli, &plan.edits, &summary, write)
}

/// `fr translate <route.ts> fastapi` — a Next.js API route as a FastAPI module.
fn cmd_translate_fastapi(cli: &Cli, path: &std::path::Path, write: bool) -> Result<()> {
    let plan = crate::transpile::nextjs::plan(path)?;
    if !cli.json {
        println!(
            "{} -> {} serving {} ({})",
            plan.source.display(),
            plan.destination.display(),
            plan.route,
            plan.methods.join(", ")
        );
        let f = &plan.fidelity;
        println!(
            "  {} handler(s), {} model(s); signatures: {} complete, {} mentioning a type \
             this tool does not know",
            f.functions, f.records, f.signatures_complete, f.signatures_with_foreign_types
        );
        if f.carried_verbatim > 0 {
            println!(
                "  {} construct(s) had no counterpart and are in the output as comments:",
                f.carried_verbatim
            );
            for note in f.notes.iter().take(10) {
                println!("    {note}");
            }
            if f.notes.len() > 10 {
                println!("    and {} more", f.notes.len() - 10);
            }
            println!(
                "\n  The routing is done: paths, methods, path parameters and models. The \n  \
                 handler bodies are TypeScript and are beside the Python to port by hand."
            );
        }
        println!();
    }
    let summary = format!(
        "{} translated to FastAPI at {}",
        plan.source.display(),
        plan.destination.display()
    );
    present(cli, &plan.edits, &summary, write)
}

/// `fr openapi` — the contract a Next.js tree declares, before it is rewritten.
fn cmd_openapi(cli: &Cli, out: Option<&std::path::Path>, yaml: bool) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let scanned = scan(&root, &ScanOptions::default())?;
    let files: Vec<PathBuf> = scanned.files.iter().map(|f| f.path.clone()).collect();

    let title = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());
    // Either side of the crossing. A Next.js tree declares nothing and the contract is
    // inferred from where the files sit; a FastAPI tree declares everything and the
    // contract is read off the decorators. Which one is here decides which is read, and
    // the report says which it was — because a document that does not say where it came
    // from cannot be argued with.
    let mut baseline = crate::openapi::from_routes(&title, &root, &files)?;
    let mut side = "Next.js route tree";
    if baseline.routes.is_empty() {
        baseline = crate::openapi::from_fastapi(&title, &root, &files)?;
        side = "FastAPI router";
    }

    if baseline.routes.is_empty() {
        anyhow::bail!(
            "no API under {}. A Next.js route is `app/**/api/**/route.ts` or anything \
             under `pages/api/`, with the URL coming from where the file sits; a FastAPI \
             one is a `@router.get(\"…\")` on an `async def`.",
            root.display()
        );
    }

    // The same document, spelled the way the reader asked for. A contract kept beside
    // the code is usually YAML, and the file this produces is meant to be read.
    let text = match yaml {
        true => serde_yaml::to_string(&baseline.document)?,
        false => serde_json::to_string_pretty(&baseline.document)?,
    };
    match out {
        Some(path) => {
            crate::vfs::write(path, format!("{text}\n"))?;
            if !cli.json {
                println!(
                    "{} route file(s) from a {side} -> {}",
                    baseline.routes.len(),
                    path.display()
                );
            }
        }
        None => println!("{text}"),
    }

    // The notes go to stderr so the document on stdout stays a document.
    if !baseline.notes.is_empty() && !cli.json {
        eprintln!(
            "\n{} thing(s) this document does not settle:",
            baseline.notes.len()
        );
        for note in &baseline.notes {
            eprintln!("  {note}");
        }
        eprintln!(
            "\nDiff this against the finished service's /openapi.json. A difference is a \n\
             defect until argued otherwise — and the one this catches is a contract that \n\
             quietly got smaller."
        );
    }
    Ok(())
}

/// `fr recipe <file>` — run a refactoring written down.
fn cmd_recipe(cli: &Cli, file: &std::path::Path, write: bool, catalogs: &[PathBuf]) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    // A relative path is relative to the workspace, as every other file argument is.
    let recipe_path = if file.is_absolute() || crate::vfs::exists(file) {
        file.to_path_buf()
    } else {
        root.join(file)
    };
    let text = crate::vfs::read_to_string(&recipe_path)
        .with_context(|| format!("reading {}", recipe_path.display()))?;
    let parsed = crate::recipe::parse(&text)?;

    let scanned = scan(&root, &ScanOptions::default())?;
    let mut sources = std::collections::BTreeMap::new();
    for source_file in &scanned.files {
        let text = crate::vfs::read_to_string(&source_file.path)?;
        sources.insert(source_file.path.clone(), (source_file.language, text));
    }

    let mut all_ok = true;
    for recipe in &parsed.recipes {
        let options = crate::recipe::Options {
            root: &root,
            catalogs,
        };
        let (report, after) = crate::recipe::run(recipe, sources.clone(), &options)?;
        // The run planned against an in-memory copy; the diff and any write are about
        // the real workspace.
        crate::vfs::use_filesystem();
        all_ok &= report.ok;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_recipe_report(&report);
        }

        // The recipe is one transaction: the diff is the whole run, not a step.
        let mut edits = crate::edit::EditSet::new();
        for (path, (_, text)) in &after {
            let before = sources.get(path).map(|(_, t)| t.as_str()).unwrap_or("");
            if before != text {
                edits.add(
                    path.clone(),
                    crate::edit::Edit::new(
                        crate::span::Span::new(0, before.len()),
                        text,
                        format!("recipe {}", report.recipe),
                    ),
                );
            }
        }
        if !cli.json {
            present(cli, &edits, &format!("recipe {}", report.recipe), write)?;
        } else if write {
            crate::edit::commit(&crate::edit::plan(
                &edits,
                crate::edit::Validation::ReparseStrict,
            )?)?;
        }
        sources = after;
    }

    if !all_ok {
        std::process::exit(1);
    }
    Ok(())
}

fn print_recipe_report(report: &crate::recipe::Report) {
    println!("recipe {} — {} step(s)", report.recipe, report.steps.len());
    if let Some(description) = &report.description {
        println!("  {description}");
    }
    println!();
    for (i, step) in report.steps.iter().enumerate() {
        println!(
            "  {}  {}{}",
            i + 1,
            step.step,
            if step.selector.is_empty() {
                String::new()
            } else {
                format!(" where {}", step.selector)
            }
        );
        println!(
            "     matched {}, applied {}, {} file(s) changed",
            step.matched, step.applied, step.files_changed
        );
        for refusal in &step.refusals {
            println!("       refused  {} — {}", refusal.subject, refusal.reason);
        }
        for warning in &step.warnings {
            println!("       left     {warning}");
        }
    }
    if !report.expectations.is_empty() {
        println!("\nexpect");
        for expectation in &report.expectations {
            println!(
                "  {} {:<24} {}",
                if expectation.held { "✓" } else { "✗" },
                expectation.expectation,
                expectation.actual
            );
        }
    }
    println!();
}

fn cmd_remove_flag(cli: &Cli, flag: &str, value: bool, write: bool) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let plan = crate::refactor::cascade::remove_flag(&root, flag, value)?;

    if plan.is_empty() {
        println!("Removing {flag} as {value} changes nothing.");
        return Ok(());
    }

    if !cli.json {
        println!("Cascade:");
        for (i, round) in plan.rounds.iter().enumerate() {
            println!(
                "  {}. {} ({} file(s))",
                i + 1,
                round.description,
                round.files_touched
            );
        }
        println!();
    }

    // A partial cascade is still useful, but only if it says what it left undone.
    if !plan.unfinished.is_empty() && !cli.json {
        println!("Left undone:");
        for item in plan.unfinished.iter().take(20) {
            println!("  {item}");
        }
        if plan.unfinished.len() > 20 {
            println!("  … and {} more", plan.unfinished.len() - 20);
        }
        println!();
    }

    let summary = format!(
        "removed {} as {} in {} round(s)",
        plan.flag,
        plan.value,
        plan.rounds.len()
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_rewrite(cli: &Cli, target: &str, name: Option<&str>, write: bool) -> Result<()> {
    use crate::refactor::rewrite::{self, Rewrite};

    let pos = parse_position(target)
        .ok_or_else(|| anyhow::anyhow!("a rewrite needs a position: path:line:col"))?;
    let path = workspace_path(cli, &pos.path)?;
    let source =
        crate::vfs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let offset = LineIndex::new(&source)
        .offset(
            LineCol {
                line: pos.line,
                col: pos.col,
            },
            &source,
        )
        .with_context(|| format!("{}:{} is outside {}", pos.line, pos.col, path.display()))?;

    let index = build_index(cli, &[])?;

    let Some(name) = name else {
        // No transformation named: list what is on offer here.
        let options = rewrite::available(&index, &path, offset)?;
        if cli.json {
            let payload: Vec<_> = options
                .iter()
                .map(|r| serde_json::json!({ "name": r.as_str(), "describe": r.describe() }))
                .collect();
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }
        if options.is_empty() {
            println!("Nothing applies at that position.");
            return Ok(());
        }
        for option in &options {
            println!("{:<14} {}", option.as_str(), option.describe());
        }
        return Ok(());
    };

    let rewrite = Rewrite::from_name(name).ok_or_else(|| {
        let known: Vec<_> = Rewrite::ALL.iter().map(|r| r.as_str()).collect();
        anyhow::anyhow!("unknown rewrite '{name}'. Known: {}", known.join(", "))
    })?;

    let plan = rewrite::apply(&index, &path, offset, rewrite)?;
    let summary = format!("{}: {}", plan.rewrite.as_str(), plan.rewrite.describe());
    present(cli, &plan.edits, &summary, write)
}

fn cmd_restructure(
    cli: &Cli,
    pattern: &str,
    template: &str,
    language: &str,
    write: bool,
) -> Result<()> {
    let lang = resolve_languages(std::slice::from_ref(&language.to_string()))?[0];
    let index = build_index(cli, &[])?;
    let plan = crate::refactor::restructure::apply(&index, lang, pattern, template)?;

    if plan.matches.is_empty() {
        println!("No {lang} code matches `{pattern}`.");
        return Ok(());
    }

    let summary = format!(
        "rewrote {} occurrence(s) of `{}` in {} file(s)",
        plan.matches.len(),
        plan.pattern,
        plan.edits.file_count()
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_flow(
    cli: &Cli,
    direction: &str,
    target: &str,
    depth: usize,
    inputs: &crate::analysis::provenance::ValuesInputs,
) -> Result<()> {
    use crate::analysis::flow;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;

    // `-f`/`--set` describe a helm invocation. Accepting them for a target no
    // values file can reach would be accepting an argument and ignoring it.
    if !inputs.is_empty() && !matches!(symbol.language, Language::Helm | Language::Yaml) {
        anyhow::bail!(
            "-f/--set describe a helm invocation, but '{}' is {}, whose values have no Helm \
             precedence to decide; drop the flags or point at a chart's values",
            symbol.name,
            symbol.language
        );
    }

    // Config and markup languages have substitution and override provenance rather
    // than dataflow, so the same command routes to whichever model applies.
    if !flow::applies_to(&index, &symbol.file) {
        use crate::analysis::provenance;
        let result = match direction {
            "back" => provenance::provenance_with_inputs(&index, symbol.id, depth, inputs)?,
            "fwd" => provenance::consumers_with_inputs(&index, symbol.id, depth, inputs)?,
            other => anyhow::bail!("unknown direction '{other}'; use 'back' or 'fwd'"),
        };

        if cli.json {
            let hops: Vec<_> = result
                .hops
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "text": h.text,
                        "file": h.file,
                        "depth": h.depth,
                        "confidence": h.confidence.as_str(),
                    })
                })
                .collect();
            let stops: Vec<_> = result
                .stops
                .iter()
                .map(|(d, r)| serde_json::json!({ "depth": d, "reason": r.to_string() }))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "symbol": symbol.qualified_name(),
                    "direction": direction,
                    "model": "provenance",
                    "values_inputs": inputs.describe(),
                    "hops": hops,
                    "competitions": result.competitions.len(),
                    "stops": stops,
                }))?
            );
        } else {
            print!("{}", result.format_tree());
            report_values_inputs(inputs, &result);
        }
        return Ok(());
    }

    let result = match direction {
        "back" => flow::backward(&index, &symbol.file, symbol.name_span.start, depth)?,
        "fwd" => flow::forward(&index, symbol.id, depth)?,
        other => anyhow::bail!("unknown direction '{other}'; use 'back' or 'fwd'"),
    };

    if cli.json {
        let steps: Vec<_> = result
            .steps
            .iter()
            .map(|s| {
                serde_json::json!({
                    "text": s.text,
                    "file": s.file,
                    "depth": s.depth,
                    "confidence": s.confidence.as_str(),
                })
            })
            .collect();
        let stops: Vec<_> = result
            .stops
            .iter()
            .map(|(depth, reason)| serde_json::json!({ "depth": depth, "reason": reason.to_string() }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": symbol.qualified_name(),
                "direction": direction,
                "steps": steps,
                "stops": stops,
            }))?
        );
    } else {
        print!("{}", result.format_tree());
    }
    Ok(())
}

/// Say which supplied input decided each Helm competition, and what the answer
/// still rests on. With nothing supplied there is nothing extra to say, and the
/// tree already reports every competition as undecided.
fn report_values_inputs(
    inputs: &crate::analysis::provenance::ValuesInputs,
    result: &crate::analysis::provenance::Provenance,
) {
    if inputs.is_empty() {
        return;
    }
    println!("\nValues inputs supplied: {}", inputs.describe());
    for competition in &result.competitions {
        match competition.winner() {
            Some(winner) => println!(
                "  {} decided by {} — {}",
                competition.subject, winner.precedence.label, winner.hop.text
            ),
            None => println!(
                "  {}: nothing supplied, and nothing in the chart, decides it",
                competition.subject
            ),
        }
    }
    let unsupplied = inputs.unsupplied();
    if !unsupplied.is_empty() {
        println!(
            "Decided given the inputs supplied: {} not listed here would change these answers.",
            unsupplied.join(" and ")
        );
    }
}

fn cmd_stitch(cli: &Cli, env: Option<&str>, orphaned_only: bool) -> Result<()> {
    use crate::analysis::stitch;

    let index = build_index(cli, &[])?;
    let mut chains = match env {
        Some(name) => stitch::for_variable(&index, name)?,
        None => stitch::chains(&index)?,
    };
    if orphaned_only {
        chains.retain(|c| c.is_orphaned());
    }

    if cli.json {
        let payload: Vec<_> = chains
            .iter()
            .map(|c| {
                serde_json::json!({
                    "env_var": c.env_var,
                    "declared_in": c.declared_in,
                    "declared_line": c.declared_line,
                    "values_path": c.values_path,
                    "values_file": c.values_file,
                    "conditional_on": c.conditional_on,
                    "reads": c.reads.iter().map(|r| serde_json::json!({
                        "file": r.file,
                        "line": r.line,
                        "language": r.language.name(),
                        "confidence": r.confidence.as_str(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if chains.is_empty() {
        println!("No configuration-to-code chains found.");
        return Ok(());
    }

    print!("{}", stitch::format_chains(&chains));
    println!(
        "{} chain(s). The link from a manifest to a program is the variable's name, \n\
         which is a string on both sides -- nothing can prove the two refer to the \n\
         same variable, so those hops are reported as name-only.",
        chains.len()
    );
    Ok(())
}

fn cmd_impact(cli: &Cli, target: &str, caller_depth: usize) -> Result<()> {
    use crate::analysis::impact;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let result = impact::analyse(&index, symbol.id, caller_depth)?;

    if cli.json {
        let items: Vec<_> = result
            .items
            .iter()
            .map(|i| {
                serde_json::json!({
                    "file": i.file,
                    "language": i.language.name(),
                    "line": i.line,
                    "col": i.col,
                    "kind": i.kind.as_str(),
                    "confidence": i.confidence.as_str(),
                    "detail": i.detail,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": symbol.qualified_name(),
                "files": result.files().len(),
                "languages": result.languages().iter().map(|l| l.name()).collect::<Vec<_>>(),
                "by_kind": result.by_kind(),
                "by_confidence": result.by_confidence(),
                "items": items,
            }))?
        );
    } else {
        print!("{}", impact::format_report(&index, &result));
    }
    Ok(())
}

fn cmd_graph(cli: &Cli, dot: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let graph = CallGraph::build(&index);

    if dot {
        print!("{}", graph.to_dot(&index));
        return Ok(());
    }

    let breakdown = graph.confidence_breakdown();
    let by_origin = graph.origin_breakdown();
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "functions": graph.node_count(),
                "calls": graph.edge_count(),
                "hierarchy_edges": graph.hierarchy_edge_count(),
                "unresolved_calls": graph.unresolved.len(),
                "by_confidence": breakdown,
                "by_origin": by_origin,
            }))?
        );
        return Ok(());
    }

    println!("functions         {}", graph.node_count());
    println!(
        "call edges        {} ({} from hierarchy analysis)",
        graph.edge_count(),
        graph.hierarchy_edge_count()
    );
    for (confidence, count) in &breakdown {
        println!("  {confidence:<16} {count}");
    }
    println!("unresolved calls  {}", graph.unresolved.len());

    // A call site the dispatch scan and the index disagree about is reported, since
    // an edge placed on the wrong offset would be worse than a missing one.
    if !graph.hierarchy_gaps.is_empty() {
        println!(
            "\n{} call site(s) the hierarchy scan could not line up with the index:",
            graph.hierarchy_gaps.len()
        );
        for (file, detail) in graph.hierarchy_gaps.iter().take(10) {
            println!("  {}: {detail}", file.display());
        }
    }
    Ok(())
}

fn cmd_entrypoints(
    cli: &Cli,
    kind_filter: Option<&str>,
    extra_catalogs: Option<&std::path::Path>,
    unreachable: bool,
) -> Result<()> {
    let index = build_index(cli, &[])?;
    let mut catalog = Catalog::builtin()?;
    if let Some(dir) = extra_catalogs {
        let added = catalog.load_dir(dir)?;
        tracing::info!("loaded {added} extra rule(s) from {}", dir.display());
    }

    let entries = catalog.detect(&index);
    let selected: Vec<_> = entries
        .iter()
        .filter(|e| kind_filter.is_none_or(|k| e.kind.as_str() == k))
        .collect();

    if unreachable {
        let graph = CallGraph::build(&index);
        let seeds: Vec<_> = entries.iter().map(|e| e.symbol).collect();
        let reachable = graph.reachable_from(&seeds);
        let orphans: Vec<_> = crate::analysis::call_graph::callables(&index)
            .into_iter()
            .filter(|s| !reachable.contains(&s.id))
            .collect();

        if cli.json {
            let payload: Vec<_> = orphans
                .iter()
                .map(|s| serde_json::json!({ "name": s.qualified_name(), "file": s.file }))
                .collect();
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            println!(
                "{} function(s) not reachable from any of {} entry point(s):",
                orphans.len(),
                entries.len()
            );
            for s in &orphans {
                println!("  {:<40} {}", s.qualified_name(), s.file.display());
            }
            println!(
                "\nNote: reachability follows resolved call edges plus class-hierarchy \
                 dispatch, so a method reached through a trait object or an interface \
                 value is counted. A function held in a map or a struct field, and a \
                 name assembled at runtime, are not — this list can still include \
                 functions that are used."
            );
        }
        return Ok(());
    }

    if cli.json {
        let payload: Vec<_> = selected
            .iter()
            .filter_map(|e| {
                index.symbol(e.symbol).map(|s| {
                    serde_json::json!({
                        "name": s.qualified_name(),
                        "file": s.file,
                        "language": s.language.name(),
                        "kind": e.kind.as_str(),
                        "rule": e.rule,
                    })
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for entry in &selected {
            if let Some(symbol) = index.symbol(entry.symbol) {
                println!(
                    "{:<18} {:<32} {}",
                    entry.kind.as_str(),
                    symbol.qualified_name(),
                    symbol.file.display()
                );
            }
        }
        println!("\n{} entry point(s)", selected.len());
        let gaps = crate::analysis::entrypoints::languages_without_rules(&catalog);
        if !gaps.is_empty() {
            println!("No entry-point rules exist for: {}", gaps.join(", "));
        }
    }
    Ok(())
}

fn cmd_rename(cli: &Cli, target: &str, new_name: &str, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let plan = crate::refactor::rename::plan(&index, symbol.id, new_name)?;

    let outcomes = crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict)?;

    if cli.json {
        let files: Vec<_> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "path": o.path,
                    "diff": o.unified_diff(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "old_name": plan.old_name,
                "new_name": plan.new_name,
                "files_changed": outcomes.len(),
                "reference_edits": plan.reference_edits,
                "applied": write,
                "changes": files,
                "warnings": plan.warnings,
            }))?
        );
        if write {
            crate::edit::commit(&outcomes)?;
        }
        return Ok(());
    }

    for outcome in &outcomes {
        print!("{}", outcome.unified_diff());
    }

    println!(
        "\n{} → {}: {} site(s) across {} file(s)",
        plan.old_name,
        plan.new_name,
        plan.reference_edits + 1,
        outcomes.len()
    );

    if !plan.warnings.is_empty() {
        let grouped = crate::refactor::rename::group_warnings(&plan.warnings);
        println!("\nNot changed — review these yourself:");
        for (kind, warnings) in grouped {
            println!("  {} ({}):", kind, warnings.len());
            for w in warnings.iter().take(10) {
                println!(
                    "    {}:{}:{}  {}",
                    w.file.display(),
                    w.line,
                    w.col,
                    w.detail
                );
            }
            if warnings.len() > 10 {
                println!("    … and {} more", warnings.len() - 10);
            }
        }
    }

    if write {
        let count = crate::edit::commit(&outcomes)?;
        println!("\nApplied to {count} file(s).");
    } else {
        println!("\nNothing written. Re-run with --write to apply.");
    }
    Ok(())
}

/// A position in a file, given as `path:line:col`.
struct Position {
    path: PathBuf,
    line: usize,
    col: usize,
}

/// Parse `path:line:col`. Returns `None` for anything else, which callers treat as
/// a bare symbol name.
fn parse_position(target: &str) -> Option<Position> {
    let mut parts = target.rsplitn(3, ':');
    let col: usize = parts.next()?.parse().ok()?;
    let line: usize = parts.next()?.parse().ok()?;
    let path = parts.next()?;
    Some(Position {
        path: PathBuf::from(path),
        line,
        col,
    })
}

/// Resolve a CLI target to a symbol, accepting either a position or a name.
fn resolve_target<'a>(cli: &Cli, index: &'a Index, target: &str) -> Result<&'a Symbol> {
    if let Some(pos) = parse_position(target) {
        let path = workspace_path(cli, &pos.path)?;
        let source = crate::vfs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let offset = LineIndex::new(&source)
            .offset(
                LineCol {
                    line: pos.line,
                    col: pos.col,
                },
                &source,
            )
            .with_context(|| format!("{}:{} is outside {}", pos.line, pos.col, path.display()))?;

        return index.definition_at(&path, offset).ok_or_else(|| {
            anyhow::anyhow!(
                "no symbol or resolved reference at {}:{}:{}",
                path.display(),
                pos.line,
                pos.col
            )
        });
    }

    // A qualified name — `Box::size`, the spelling every listing prints — before a bare
    // one. The tool printed these everywhere and then refused them as input, so the
    // obvious way to name one of twenty `String` methods was the one way that did not
    // work, and the only alternative offered was a line and column somebody had to go
    // and look up.
    let matches = match target.contains("::") {
        true => index
            .symbols
            .iter()
            .filter(|s| s.qualified_name() == target)
            .collect::<Vec<_>>(),
        false => Vec::new(),
    };
    if matches.len() == 1 {
        return Ok(matches[0]);
    }
    if matches.len() > 1 {
        let mut listing = String::new();
        for symbol in &matches {
            listing.push_str(&format!("\n  {} in {}", target, symbol.file.display()));
        }
        anyhow::bail!(
            "'{target}' is declared in {} files; specify a position as \
             path:line:col{listing}",
            matches.len()
        );
    }

    let matches = index.find_symbols(target, None);
    match matches.len() {
        0 => anyhow::bail!("no symbol named '{target}'"),
        1 => Ok(matches[0]),
        // Several sites can declare one entity — a CSS class has no canonical
        // definition — and that is not an ambiguous choice between rivals.
        _ if index.is_one_entity(&matches) => Ok(matches[0]),
        _ => {
            // Ambiguity is reported, never resolved by guessing.
            let mut listing = String::new();
            // Each candidate is listed by the name that would select it, so the fix is
            // to copy a line rather than to go and find a line number.
            for symbol in &matches {
                listing.push_str(&format!(
                    "\n  {} ({}) in {}",
                    symbol.qualified_name(),
                    symbol.kind.as_str(),
                    symbol.file.display()
                ));
            }
            anyhow::bail!(
                "'{target}' is defined {} times; name one of these, or give a position \
                 as path:line:col{listing}",
                matches.len()
            )
        }
    }
}

fn build_index(cli: &Cli, languages: &[String]) -> Result<Index> {
    let options = scan_options(languages)?;
    // Canonicalise the root so indexed paths match the ones commands resolve from
    // arguments; otherwise /var and /private/var name the same file but never match.
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let scanned = crate::scan::scan(&root, &options)?;

    let cache = if cli.no_cache {
        None
    } else {
        crate::cache::Cache::open()
    };
    let index = Index::build_with_cache(&scanned, cache.as_ref())?;

    if let Some(cache) = &cache {
        let (hits, misses) = cache.stats();
        tracing::debug!("cache: {hits} hit(s), {misses} miss(es)");
    }
    Ok(index)
}

fn cmd_capabilities(
    cli: &Cli,
    capability: Option<&str>,
    language: Option<&str>,
    markdown: bool,
) -> Result<()> {
    use crate::capabilities::{self, Capability};

    if markdown {
        print!("{}", capabilities::render_markdown());
        return Ok(());
    }

    let wanted_language = match language {
        Some(name) => Some(resolve_languages(&[name.to_string()])?[0]),
        None => None,
    };
    let rows: Vec<_> = capabilities::matrix()
        .into_iter()
        .filter(|r| {
            capability.is_none_or(|c| r.capability.replace(' ', "-") == c || r.capability == c)
        })
        .collect();

    if rows.is_empty() {
        let known: Vec<_> = Capability::ALL.iter().map(|c| c.as_str()).collect();
        anyhow::bail!("unknown capability. Known: {}", known.join(", "));
    }

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    for row in &rows {
        println!("{}  ({})", row.capability, row.command);
        for (name, support) in &row.languages {
            if wanted_language.is_some_and(|l| l.name() != *name) {
                continue;
            }
            match support.reason() {
                Some(why) => println!("  {:<4} {:<11} {why}", support.mark(), name),
                None => println!("  {:<4} {name}", support.mark()),
            }
        }
        println!();
    }

    let (yes, not_applicable, refused) = capabilities::totals();
    println!(
        "{yes} supported, {not_applicable} not applicable, {refused} refused \n\
         (of {} capability x language pairs)",
        yes + not_applicable + refused
    );
    Ok(())
}

fn cmd_cache(cli: &Cli, clear: bool) -> Result<()> {
    let Some(cache) = crate::cache::Cache::open() else {
        println!(
            "No cache location is available, so every command re-reads each file. Set \
             FUN_REFACTOR_CACHE or XDG_CACHE_HOME to enable it."
        );
        return Ok(());
    };

    if clear {
        cache.clear()?;
        println!("Cleared {}.", cache.location().display());
        return Ok(());
    }

    let bytes = cache.size_bytes();
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "location": cache.location(),
                "bytes": bytes,
            }))?
        );
    } else {
        println!("location  {}", cache.location().display());
        println!("size      {} KiB", bytes / 1024);
        println!(
            "\nEntries are keyed by file content and by the query set, so editing a \n\
             query file makes every stale entry unreachable rather than wrong."
        );
    }
    Ok(())
}

fn cmd_symbols(
    cli: &Cli,
    languages: &[String],
    name_filter: Option<&str>,
    kind_filter: Option<&str>,
    stats: bool,
) -> Result<()> {
    let index = build_index(cli, languages)?;

    if stats {
        let s = index.stats();
        if cli.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "files": s.files,
                    "symbols": s.symbols,
                    "references": s.references,
                    "resolved": s.resolved,
                    "by_confidence": s.by_confidence,
                    "files_with_parse_errors": s.files_with_parse_errors,
                }))?
            );
        } else {
            println!("files       {}", s.files);
            println!("symbols     {}", s.symbols);
            println!("references  {} ({} resolved)", s.references, s.resolved);
            for (confidence, count) in &s.by_confidence {
                println!("  {confidence:<18} {count}");
            }
            if s.files_with_parse_errors > 0 {
                println!(
                    "\n{} file(s) had parse errors; their facts may be incomplete",
                    s.files_with_parse_errors
                );
            }
        }
        return Ok(());
    }

    let selected: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| name_filter.is_none_or(|n| s.name.contains(n)))
        .filter(|s| kind_filter.is_none_or(|k| s.kind.as_str() == k))
        .collect();

    if cli.json {
        let payload: Vec<_> = selected
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "qualified_name": s.qualified_name(),
                    "kind": s.kind.as_str(),
                    "file": s.file,
                    "language": s.language.name(),
                    "exported": s.exported,
                    "name_span": { "start": s.name_span.start, "end": s.name_span.end },
                    "full_span": { "start": s.full_span.start, "end": s.full_span.end },
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for symbol in &selected {
            println!(
                "{:<12} {:<30} {}",
                symbol.kind.as_str(),
                symbol.qualified_name(),
                symbol.file.display()
            );
        }
        println!("\n{} symbol(s)", selected.len());
    }
    Ok(())
}

fn cmd_type(cli: &Cli, target: &str) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let declared = crate::analysis::types::of(&index, symbol.id)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&declared)?);
        return Ok(());
    }

    println!("{}  {}", declared.name, declared.describe());
    if let Some(inferred) = &declared.inferred {
        if let Some(from) = inferred.from.and_then(|id| index.symbol(id)) {
            let source = crate::vfs::read_to_string(&from.file).unwrap_or_default();
            let at = crate::span::LineIndex::new(&source).line_col(from.name_span.start, &source);
            println!(
                "  evidence: {} at {}:{}",
                from.name,
                from.file.display(),
                at
            );
        }
    }
    for (name, ty) in &declared.parameters {
        match ty {
            Some(ty) => println!("  {name}: {ty}"),
            None => println!("  {name}: no type written down"),
        }
    }
    if let Some(defined) = declared.defined_at.and_then(|id| index.symbol(id)) {
        let source = crate::vfs::read_to_string(&defined.file).unwrap_or_default();
        let at = crate::span::LineIndex::new(&source).line_col(defined.name_span.start, &source);
        println!(
            "\n{} is defined at {}:{}",
            declared.describe(),
            defined.file.display(),
            at
        );
    }
    match (&declared.declared, &declared.inferred) {
        (None, None) => println!(
            "\nThe source wrote no type here, and nothing follows from what it did \n\
             write. That is the answer rather than a gap in one."
        ),
        (None, Some(_)) => println!(
            "\nThe source wrote no type here. The above was worked out from the \n\
             evidence named, and is a derivation rather than a contract."
        ),
        _ => {}
    }
    Ok(())
}

fn cmd_def(cli: &Cli, target: &str, first_only: bool) -> Result<()> {
    use crate::navigate;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let found = navigate::definitions_of(&index, symbol.id);

    if cli.json {
        let payload: Vec<_> = found
            .definitions
            .iter()
            .filter(|d| !first_only || d.role == navigate::DefinitionRole::Primary)
            .map(|d| {
                serde_json::json!({
                    "name": d.name,
                    "qualified_name": d.qualified_name,
                    "kind": d.kind.as_str(),
                    "role": d.role.as_str(),
                    "file": d.location.file,
                    "line": d.location.line,
                    "col": d.location.col,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    for definition in &found.definitions {
        if first_only && definition.role != navigate::DefinitionRole::Primary {
            continue;
        }
        println!(
            "{:<18} {:<12} {}:{}:{}",
            definition.role.as_str(),
            definition.kind.as_str(),
            definition.location.file.display(),
            definition.location.line,
            definition.location.col
        );
        if !definition.location.preview.is_empty() {
            println!("                   {}", definition.location.preview);
        }
    }

    if found.is_polymorphic() && !first_only {
        println!(
            "\n`{}` is declared on an abstraction, so which one runs is a runtime \n\
             fact. Every implementation is listed.",
            found.query
        );
    }
    Ok(())
}

fn cmd_implementations(cli: &Cli, target: &str) -> Result<()> {
    use crate::navigate;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let found = navigate::implementations_of(&index, symbol.id);

    if found.is_empty() {
        println!(
            "`{}` has no implementations: nothing declares it as an abstraction.",
            symbol.qualified_name()
        );
        return Ok(());
    }

    let rendered: Vec<_> = found
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| (s.qualified_name(), s.file.clone()))
        .collect();

    if cli.json {
        let payload: Vec<_> = rendered
            .iter()
            .map(|(name, file)| serde_json::json!({ "name": name, "file": file }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    for (name, file) in &rendered {
        println!("{:<34} {}", name, file.display());
    }
    println!(
        "\n{} implementation(s). These are dispatch candidates, matched through \n\
         declared implements-relationships — which one runs is a runtime fact.",
        rendered.len()
    );
    Ok(())
}

fn cmd_usages(cli: &Cli, target: &str, include_unresolved: bool) -> Result<()> {
    use crate::navigate;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let found = navigate::usages_of(&index, symbol.id);

    if cli.json {
        let render = |list: &[&navigate::Usage]| {
            list.iter()
                .map(|u| {
                    serde_json::json!({
                        "file": u.location.file,
                        "line": u.location.line,
                        "col": u.location.col,
                        "within": u.within,
                        "confidence": u.confidence.as_str(),
                        "preview": u.location.preview,
                    })
                })
                .collect::<Vec<_>>()
        };
        let all: Vec<&navigate::Usage> = found.usages.iter().collect();
        let weak: Vec<&navigate::Usage> = found.same_name_elsewhere.iter().collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": found.query,
                "usages": render(&all),
                "same_name_elsewhere": if include_unresolved { render(&weak) } else { Vec::new() },
            }))?
        );
        return Ok(());
    }

    for (file, usages) in found.by_file() {
        println!("{}", file.display());
        for usage in usages {
            let context = usage
                .within
                .as_deref()
                .map(|w| format!("  in {w}"))
                .unwrap_or_default();
            let confidence = if usage.confidence.is_safe_to_rewrite() {
                String::new()
            } else {
                format!("  [{}]", usage.confidence.as_str())
            };
            println!(
                "  {}:{}  {}{context}{confidence}",
                usage.location.line, usage.location.col, usage.location.preview
            );
        }
    }
    println!("\n{} use(s) of {}", found.usages.len(), found.query);

    if include_unresolved && !found.same_name_elsewhere.is_empty() {
        println!(
            "\n{} occurrence(s) of the same name that did NOT resolve here:",
            found.same_name_elsewhere.len()
        );
        for usage in found.same_name_elsewhere.iter().take(20) {
            println!(
                "  {}:{}:{}  [{}]",
                usage.location.file.display(),
                usage.location.line,
                usage.location.col,
                usage.confidence.as_str()
            );
        }
    }
    Ok(())
}

fn cmd_refs(cli: &Cli, target: &str, include_unresolved: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let refs = index.references_to(symbol.id);
    let weak = if include_unresolved {
        index.unresolved_matching(symbol.id)
    } else {
        Vec::new()
    };

    // Line/column lookups need each file's text; read once per file.
    let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut locate = |file: &PathBuf, offset: usize| -> (usize, usize) {
        let source = sources
            .entry(file.clone())
            .or_insert_with(|| crate::vfs::read_to_string(file).unwrap_or_default());
        let pos = LineIndex::new(source).line_col(offset, source);
        (pos.line, pos.col)
    };

    if cli.json {
        let render =
            |list: &[&crate::model::Reference],
             locate: &mut dyn FnMut(&PathBuf, usize) -> (usize, usize)| {
                list.iter()
                    .map(|r| {
                        let (line, col) = locate(&r.file, r.span.start);
                        serde_json::json!({
                            "file": r.file,
                            "line": line,
                            "col": col,
                            "kind": format!("{:?}", r.kind).to_lowercase(),
                            "confidence": r.confidence.as_str(),
                        })
                    })
                    .collect::<Vec<_>>()
            };
        let resolved = render(&refs, &mut locate);
        let unresolved = render(&weak, &mut locate);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": symbol.qualified_name(),
                "references": resolved,
                "same_name_elsewhere": unresolved,
            }))?
        );
        return Ok(());
    }

    println!("{} reference(s) to {}", refs.len(), symbol.qualified_name());
    for r in &refs {
        let (line, col) = locate(&r.file, r.span.start);
        println!(
            "  {}:{}:{}  [{}]",
            r.file.display(),
            line,
            col,
            r.confidence.as_str()
        );
    }

    if include_unresolved && !weak.is_empty() {
        println!(
            "\n{} occurrence(s) of the same name that did NOT resolve here:",
            weak.len()
        );
        for r in &weak {
            let (line, col) = locate(&r.file, r.span.start);
            println!(
                "  {}:{}:{}  [{}]",
                r.file.display(),
                line,
                col,
                r.confidence.as_str()
            );
        }
    }
    Ok(())
}

/// Resolve `--lang` values, failing loudly on an unknown name rather than silently
/// scanning everything.
fn resolve_languages(names: &[String]) -> Result<Vec<Language>> {
    names
        .iter()
        .map(|n| {
            Language::from_name(n).ok_or_else(|| {
                let known: Vec<_> = Language::ALL.iter().map(|l| l.name()).collect();
                anyhow::anyhow!(
                    "unknown language '{n}'. Known languages: {}",
                    known.join(", ")
                )
            })
        })
        .collect()
}

fn scan_options(names: &[String]) -> Result<ScanOptions> {
    Ok(ScanOptions {
        languages: resolve_languages(names)?,
        ..Default::default()
    })
}

fn cmd_scan(cli: &Cli, languages: &[String]) -> Result<()> {
    let options = scan_options(languages)?;
    let result = scan(&cli.root, &options)?;

    if cli.json {
        let files: Vec<_> = result
            .files
            .iter()
            .map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "language": f.language.name(),
                })
            })
            .collect();
        let skipped: Vec<_> = result
            .skipped_too_large
            .iter()
            .map(|(p, size)| serde_json::json!({ "path": p, "bytes": size }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "files": files,
                "skipped_too_large": skipped,
            }))?
        );
    } else {
        for file in &result.files {
            println!("{:<10} {}", file.language.name(), file.path.display());
        }
        println!("\n{} file(s)", result.files.len());
        report_skipped(&result);
    }
    Ok(())
}

fn cmd_parse(cli: &Cli, languages: &[String], stats: bool) -> Result<()> {
    let options = scan_options(languages)?;
    let result = scan(&cli.root, &options)?;
    let parsers = Parsers::new();

    #[derive(Default, Clone, Copy)]
    struct Tally {
        files: usize,
        with_errors: usize,
        error_nodes: usize,
        unreadable: usize,
    }

    let mut per_language: BTreeMap<&'static str, Tally> = BTreeMap::new();
    // The count and where. A file named with "2 error node(s)" and no position is a
    // report somebody cannot act on: the whole value of knowing a file did not parse is
    // being able to go and look at the part that did not.
    let mut failures: Vec<(PathBuf, Vec<LineCol>)> = Vec::new();

    for file in &result.files {
        let tally = per_language.entry(file.language.name()).or_default();
        tally.files += 1;

        let Ok(source) = crate::vfs::read_to_string(&file.path) else {
            // Not valid UTF-8, or unreadable: counted and reported, never ignored.
            tally.unreadable += 1;
            continue;
        };
        let parsed = parsers.parse(file.language, &source)?;
        let spans = parsed.error_spans();
        if !spans.is_empty() {
            tally.with_errors += 1;
            tally.error_nodes += spans.len();
            let lines = LineIndex::new(&source);
            let at = spans
                .iter()
                .map(|span| lines.line_col(span.start, &source))
                .collect();
            failures.push((file.path.clone(), at));
        }
    }

    if cli.json {
        let langs: Vec<_> = per_language
            .iter()
            .map(|(name, t)| {
                serde_json::json!({
                    "language": name,
                    "files": t.files,
                    "files_with_errors": t.with_errors,
                    "error_nodes": t.error_nodes,
                    "unreadable": t.unreadable,
                })
            })
            .collect();
        let mut payload = serde_json::json!({ "languages": langs });
        if !stats {
            payload["files_with_errors"] = serde_json::json!(failures
                .iter()
                .map(|(p, at)| {
                    serde_json::json!({
                        "path": p,
                        "error_nodes": at.len(),
                        "at": at
                            .iter()
                            .map(|pos| serde_json::json!({ "line": pos.line, "col": pos.col }))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>());
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!(
        "{:<12} {:>7} {:>8} {:>8}",
        "LANGUAGE", "FILES", "ERRORS", "UNREAD"
    );
    for (name, t) in &per_language {
        println!(
            "{:<12} {:>7} {:>8} {:>8}",
            name, t.files, t.with_errors, t.unreadable
        );
    }

    if !stats && !failures.is_empty() {
        println!("\nFiles with parse errors:");
        for (path, at) in &failures {
            // Every position, up to a handful. A file with two hundred error nodes is
            // one the grammar cannot read at all, and listing them would say that two
            // hundred times.
            let shown: Vec<String> = at.iter().take(4).map(|pos| pos.to_string()).collect();
            let more = match at.len() > shown.len() {
                true => format!(", and {} more", at.len() - shown.len()),
                false => String::new(),
            };
            println!(
                "  {}:{}{more}  ({} error node(s))",
                path.display(),
                shown.join(", "),
                at.len()
            );
        }
    }
    report_skipped(&result);
    Ok(())
}

fn report_skipped(result: &crate::scan::ScanResult) {
    if !result.skipped_too_large.is_empty() {
        println!(
            "\n{} file(s) skipped for exceeding the size limit:",
            result.skipped_too_large.len()
        );
        for (path, size) in &result.skipped_too_large {
            println!("  {} ({} bytes)", path.display(), size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_language_is_rejected_with_guidance() {
        let err = resolve_languages(&["kotlin".to_string()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown language 'kotlin'"));
        assert!(err.contains("rust"), "error should list known languages");
    }

    #[test]
    fn known_languages_resolve() {
        let langs = resolve_languages(&["rust".into(), "tsx".into()]).unwrap();
        assert_eq!(langs, vec![Language::Rust, Language::Tsx]);
    }

    #[test]
    fn cli_parses_expected_invocations() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
