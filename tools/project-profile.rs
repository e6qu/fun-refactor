use anyhow::{Context, Result};
use clap::Parser;
use fun_refactor::cache::Cache;
use fun_refactor::index::Index;
use fun_refactor::project::{Command, Project};
use fun_refactor::scan::{scan, ScanOptions};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
struct Options {
    #[arg(short = 'C')]
    root: PathBuf,
    #[arg(long)]
    no_cache: bool,
    #[arg(long)]
    construction: bool,
    #[command(subcommand)]
    command: Command,
}

fn timed<T>(
    phases: &mut BTreeMap<&str, f64>,
    name: &'static str,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let started = Instant::now();
    let value = action()?;
    phases.insert(name, started.elapsed().as_secs_f64());
    Ok(value)
}

fn main() -> Result<()> {
    let options = Options::parse();
    let mut phases = BTreeMap::new();
    let started = Instant::now();
    let root = timed(&mut phases, "root", || Ok(options.root.canonicalize()?))?;
    anyhow::ensure!(root.is_dir(), "Choose a project directory.");
    let scan_options = ScanOptions::default();
    let scanned = timed(&mut phases, "scan", || scan(&root, &scan_options))?;
    let cache = timed(&mut phases, "cache_open", || {
        if options.no_cache {
            Ok(None)
        } else {
            Ok(Some(
                Cache::open().context("Could not open the profiling cache.")?,
            ))
        }
    })?;
    let index = timed(&mut phases, "index", || {
        Index::build_with_cache(&scanned, cache.as_ref())
    })?;
    let (project, construction_seconds) = timed(&mut phases, "project", || {
        if options.construction {
            Project::new_profiled(&root, &index, &scanned, &scan_options)
        } else {
            Ok((
                Project::new(&root, &index, &scanned, &scan_options)?,
                BTreeMap::new(),
            ))
        }
    })?;
    let report = timed(&mut phases, "query", || project.report(&options.command))?;
    timed(&mut phases, "verify", || project.verify(&root))?;
    let report_stdout = timed(&mut phases, "serialize", || {
        Ok(serde_json::to_string(&report)? + "\n")
    })?;
    let fact_cache_hits = cache.as_ref().map(|cache| cache.stats().hits);
    let resolution_cache_hits = cache.as_ref().map(|cache| cache.stats().resolution_hits);
    let resolution_cache_misses = cache.as_ref().map(|cache| cache.stats().resolution_misses);
    let indexed_files = index.file_count();
    let mut references_by_language = BTreeMap::<&str, usize>::new();
    let mut references_by_file = BTreeMap::<PathBuf, usize>::new();
    for reference in &index.references {
        *references_by_language
            .entry(reference.language.name())
            .or_default() += 1;
        *references_by_file
            .entry(reference.file.clone())
            .or_default() += 1;
    }
    let mut largest_reference_files = references_by_file.into_iter().collect::<Vec<_>>();
    largest_reference_files.sort_by(|(left_path, left_count), (right_path, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_path.cmp(right_path))
    });
    largest_reference_files.truncate(20);
    let largest_reference_files = largest_reference_files
        .into_iter()
        .map(|(path, count)| {
            (
                path.strip_prefix(&root).unwrap_or(&path).to_path_buf(),
                count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let reference_count = index.references.len();
    let dropping = Instant::now();
    drop(report);
    drop(project);
    drop(index);
    drop(cache);
    drop(scanned);
    phases.insert("drop", dropping.elapsed().as_secs_f64());
    let measured_seconds = started.elapsed().as_secs_f64();
    println!(
        "{}",
        json!({"schema": "fr-project-profile-1", "report_stdout": report_stdout,
        "phases_seconds": phases, "measured_seconds": measured_seconds,
        "fact_cache_hits": fact_cache_hits, "indexed_files": indexed_files,
        "resolution_cache_hits": resolution_cache_hits,
        "resolution_cache_misses": resolution_cache_misses,
        "reference_count": reference_count, "references_by_language": references_by_language,
        "largest_reference_files": largest_reference_files,
        "construction_seconds": construction_seconds})
    );
    Ok(())
}
