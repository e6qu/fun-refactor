//! Command-line surface.

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
    }
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
    Index::build(&cli.root, &options)
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
