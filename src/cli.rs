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

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    /// Show the definition a position or name refers to.
    Def {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
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
    /// Extract an expression into a named binding.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Extract {
        /// The expression to extract, as `path:line:col-line:col`.
        range: String,
        /// Name for the new binding.
        name: String,
        /// Replace every identical occurrence in the same block.
        #[arg(long)]
        all: bool,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Replace a variable's uses with its value and remove the binding.
    ///
    /// Prints a diff by default; pass --write to apply it.
    Inline {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Trace where a value comes from or goes to.
    Flow {
        /// Direction: `back` (where does it come from) or `fwd` (where is it used).
        #[arg(value_parser = ["back", "fwd"])]
        direction: String,
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// How many hops to follow.
        #[arg(long, default_value = "5")]
        depth: usize,
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
        Command::Scan { languages } => cmd_scan(&cli, languages),
        Command::Parse { languages, stats } => cmd_parse(&cli, languages, *stats),
        Command::Symbols {
            languages,
            name,
            kind,
            stats,
        } => cmd_symbols(&cli, languages, name.as_deref(), kind.as_deref(), *stats),
        Command::Def { target } => cmd_def(&cli, target),
        Command::Refs {
            target,
            include_unresolved,
        } => cmd_refs(&cli, target, *include_unresolved),
        Command::Rename {
            target,
            new_name,
            write,
        } => cmd_rename(&cli, target, new_name, *write),
        Command::Callers { target, depth } => {
            cmd_trace(&cli, target, *depth, Direction2::Callers)
        }
        Command::Callees { target, depth } => {
            cmd_trace(&cli, target, *depth, Direction2::Callees)
        }
        Command::Flow {
            direction,
            target,
            depth,
        } => cmd_flow(&cli, direction, target, *depth),
        Command::Extract {
            range,
            name,
            all,
            write,
        } => cmd_extract(&cli, range, name, *all, *write),
        Command::Inline { target, write } => cmd_inline(&cli, target, *write),
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
    let symbol = resolve_target(&index, target)?;
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
fn present(
    cli: &Cli,
    edits: &crate::edit::EditSet,
    summary: &str,
    write: bool,
) -> Result<()> {
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

/// Parse `path:line:col-line:col` into a byte span.
fn parse_range(spec: &str) -> Result<(PathBuf, LineCol, LineCol)> {
    let (head, end_col) = spec
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected path:line:col-line:col, got '{spec}'"))?;
    let (head, end_line) = head
        .rsplit_once('-')
        .ok_or_else(|| anyhow::anyhow!("expected path:line:col-line:col, got '{spec}'"))?;
    let (path, start_col) = head
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected path:line:col-line:col, got '{spec}'"))?;
    let (path, start_line) = path
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected path:line:col-line:col, got '{spec}'"))?;

    Ok((
        PathBuf::from(path),
        LineCol {
            line: start_line.parse()?,
            col: start_col.parse()?,
        },
        LineCol {
            line: end_line.parse()?,
            col: end_col.parse()?,
        },
    ))
}

fn cmd_extract(cli: &Cli, range: &str, name: &str, all: bool, write: bool) -> Result<()> {
    let (path, start, end) = parse_range(range)?;
    let path = path.canonicalize().unwrap_or(path);
    let source = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
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
    let plan = crate::refactor::extract::variable(&index, &path, span, name, all)?;
    let summary = format!(
        "extracted `{}` into {} ({} occurrence(s) replaced)",
        plan.expression.trim(),
        plan.name,
        plan.occurrences
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_inline(cli: &Cli, target: &str, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(&index, target)?;
    let plan = crate::refactor::inline::variable(&index, symbol.id)?;
    let summary = format!(
        "inlined `{}` into {} use site(s)",
        plan.name, plan.use_sites
    );
    present(cli, &plan.edits, &summary, write)
}

fn cmd_flow(cli: &Cli, direction: &str, target: &str, depth: usize) -> Result<()> {
    use crate::analysis::flow;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(&index, target)?;

    if !flow::applies_to(&index, &symbol.file) {
        anyhow::bail!(
            "{} is a {} file. Dataflow applies to imperative languages; config and \
             markup languages have substitution and override provenance instead, \
             which is not implemented yet.",
            symbol.file.display(),
            symbol.language
        );
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

fn cmd_impact(cli: &Cli, target: &str, caller_depth: usize) -> Result<()> {
    use crate::analysis::impact;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(&index, target)?;
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
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "functions": graph.node_count(),
                "calls": graph.edge_count(),
                "unresolved_calls": graph.unresolved.len(),
                "by_confidence": breakdown,
            }))?
        );
    } else {
        println!("functions         {}", graph.node_count());
        println!("call edges        {}", graph.edge_count());
        for (confidence, count) in &breakdown {
            println!("  {confidence:<16} {count}");
        }
        println!("unresolved calls  {}", graph.unresolved.len());
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
                "\nNote: reachability follows resolved call edges only. Dynamic \
                 dispatch and calls through unresolved names are not counted, so \
                 this list can include functions that are used."
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
    let symbol = resolve_target(&index, target)?;
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
                println!("    {}:{}:{}  {}", w.file.display(), w.line, w.col, w.detail);
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
fn resolve_target<'a>(index: &'a Index, target: &str) -> Result<&'a Symbol> {
    if let Some(pos) = parse_position(target) {
        let path = pos.path.canonicalize().unwrap_or(pos.path.clone());
        let source = std::fs::read_to_string(&path)
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
            for symbol in &matches {
                listing.push_str(&format!(
                    "\n  {} ({}) in {}",
                    symbol.name,
                    symbol.kind.as_str(),
                    symbol.file.display()
                ));
            }
            anyhow::bail!(
                "'{target}' is defined {} times; specify a position as path:line:col{listing}",
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
    Index::build(&root, &options)
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

fn cmd_def(cli: &Cli, target: &str) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(&index, target)?;
    let source = std::fs::read_to_string(&symbol.file)?;
    let pos = LineIndex::new(&source).line_col(symbol.name_span.start, &source);

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": symbol.name,
                "qualified_name": symbol.qualified_name(),
                "kind": symbol.kind.as_str(),
                "file": symbol.file,
                "line": pos.line,
                "col": pos.col,
                "exported": symbol.exported,
            }))?
        );
    } else {
        println!(
            "{} {} at {}:{}:{}",
            symbol.kind.as_str(),
            symbol.qualified_name(),
            symbol.file.display(),
            pos.line,
            pos.col
        );
    }
    Ok(())
}

fn cmd_refs(cli: &Cli, target: &str, include_unresolved: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(&index, target)?;
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
            .or_insert_with(|| std::fs::read_to_string(file).unwrap_or_default());
        let pos = LineIndex::new(source).line_col(offset, source);
        (pos.line, pos.col)
    };

    if cli.json {
        let render = |list: &[&crate::model::Reference],
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
                anyhow::anyhow!("unknown language '{n}'. Known languages: {}", known.join(", "))
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
    let mut failures: Vec<(PathBuf, usize)> = Vec::new();

    for file in &result.files {
        let tally = per_language.entry(file.language.name()).or_default();
        tally.files += 1;

        let Ok(source) = std::fs::read_to_string(&file.path) else {
            // Not valid UTF-8, or unreadable: counted and reported, never ignored.
            tally.unreadable += 1;
            continue;
        };
        let parsed = parsers.parse(file.language, &source)?;
        let errors = parsed.error_spans().len();
        if errors > 0 {
            tally.with_errors += 1;
            tally.error_nodes += errors;
            failures.push((file.path.clone(), errors));
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
                .map(|(p, n)| serde_json::json!({ "path": p, "error_nodes": n }))
                .collect::<Vec<_>>());
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("{:<12} {:>7} {:>8} {:>8}", "LANGUAGE", "FILES", "ERRORS", "UNREAD");
    for (name, t) in &per_language {
        println!(
            "{:<12} {:>7} {:>8} {:>8}",
            name, t.files, t.with_errors, t.unreadable
        );
    }

    if !stats && !failures.is_empty() {
        println!("\nFiles with parse errors:");
        for (path, count) in &failures {
            println!("  {} ({} error node(s))", path.display(), count);
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
