//! Command-line surface.

use crate::lang::Language;
use crate::parse::Parsers;
use crate::scan::{scan, ScanOptions};
use anyhow::Result;
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
    }
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
