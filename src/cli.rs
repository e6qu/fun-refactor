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
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Whether [`emit`] ends the write with a newline.
enum Newline {
    Yes,
    No,
}

/// Write to stdout, taking a closed pipe as the reader saying it has enough.
fn emit(newline: Newline, text: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let result = out.write_fmt(text).and_then(|()| match newline {
        Newline::Yes => out.write_all(b"\n"),
        Newline::No => Ok(()),
    });
    match result {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(e) => panic!("failed printing to stdout: {e}"),
    }
}

// These two shadow the standard macros for the rest of this file.
macro_rules! println {
    () => { emit(Newline::Yes, format_args!("")) };
    ($($arg:tt)*) => { emit(Newline::Yes, format_args!($($arg)*)) };
}
macro_rules! print {
    ($($arg:tt)*) => { emit(Newline::No, format_args!($($arg)*)) };
}

/// The tail of `fr --help`, naming the exit codes a script can branch on.
const EXIT_CODES_HELP: &str = "Exit codes:\n  \
     0  success.\n  \
     1  failure without a more specific code below.\n  \
     2  the command line itself was invalid.\n  \
     3  nothing matched the target.\n  \
     4  the target is ambiguous; the error lists the candidates.\n  \
     5  the refactoring refused to proceed; the error says why.";

#[derive(Parser)]
#[command(
    name = "fr",
    version,
    about = "Multi-language refactoring and code intelligence",
    long_about = None,
    after_long_help = EXIT_CODES_HELP
)]
struct Cli {
    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    /// Skip files larger than this many bytes (default 4194304, 4 MiB).
    #[arg(long, global = true, value_name = "BYTES")]
    max_file_size: Option<u64>,

    /// Workspace root to operate on.
    #[arg(long, short = 'C', global = true, default_value = ".")]
    root: PathBuf,

    /// True where the caller left `root` unstated and the tool had to find it.
    #[arg(skip)]
    root_inferred: bool,

    /// Read files that .gitignore and friends exclude, and hidden files too.
    #[arg(long, global = true)]
    no_ignore: bool,

    /// Re-read every file instead of reusing cached facts.
    #[arg(long, global = true)]
    no_cache: bool,

    #[command(subcommand)]
    command: Command,
}

/// Every subcommand's name, asked of the parser and not written down again.
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
        /// Only this capability, e.g.
        #[arg(long)]
        capability: Option<String>,
        /// Only this language.
        #[arg(long = "lang", alias = "language")]
        language: Option<String>,
        /// Emit the markdown table used in the README.
        #[arg(long)]
        markdown: bool,
    },
    /// Print a shell completion script for this tool.
    Completions {
        /// The shell to write for.
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    /// Inspect or clear the fact cache.
    Cache {
        /// Delete every cached entry for the current query set.
        #[arg(long)]
        clear: bool,
    },
    /// List the source files fun-refactor can act on.
    Scan {
        /// Restrict to a language (repeatable), e.g.
        #[arg(long = "lang", alias = "language")]
        languages: Vec<String>,
    },
    /// Parse files and report syntax health.
    Parse {
        /// Restrict to a language (repeatable).
        #[arg(long = "lang", alias = "language")]
        languages: Vec<String>,
        /// Show per-language totals instead of per-file detail.
        #[arg(long)]
        stats: bool,
    },
    /// List defined symbols.
    Symbols {
        /// Restrict to a language (repeatable).
        #[arg(long = "lang", alias = "language")]
        languages: Vec<String>,
        /// Only symbols whose name contains this string.
        #[arg(long)]
        name: Option<String>,
        /// Only symbols of this kind, e.g.
        #[arg(long)]
        kind: Option<String>,
        /// Show index-wide totals instead of listing symbols.
        #[arg(long)]
        stats: bool,
        /// Only symbols in these files or directories.
        paths: Vec<PathBuf>,
    },
    /// Show every place a symbol is defined.
    Def {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Show only the primary definition.
        #[arg(long)]
        first: bool,
    },
    /// Show a symbol's type: what the source declared, or what follows from what it did.
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
    Inline {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Inline the call at that position instead of a variable.
        #[arg(long)]
        call: bool,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Change a function's parameters and update every call site.
    Signature {
        /// Position as `path:line:col`, or a bare function name.
        target: String,
        /// `remove:<i>`, `move:<from>:<to>`, or `add:<i>:<declaration>:<argument>`.
        #[arg(verbatim_doc_comment)]
        change: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Move a top-level symbol to another file, updating imports.
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
    Delete {
        /// Position as `path:line:col`, or a bare symbol name.
        target: String,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Find code written more than once.
    Duplicates {
        /// Smallest duplicate to report, in tokens.
        #[arg(long)]
        min_tokens: Option<usize>,
        /// Require identifiers and literals to match too, not only the structure.
        #[arg(long)]
        exact: bool,
        /// Only report duplicates in this language.
        #[arg(long = "lang", alias = "language", value_name = "LANG")]
        languages: Vec<String>,
        /// Only report duplicates under this path prefix.
        #[arg(long = "path", value_name = "PREFIX")]
        paths: Vec<PathBuf>,
    },
    /// List symbols nothing appears to use.
    Unused {
        /// Additional catalog directory for entry-point rules.
        #[arg(long)]
        catalogs: Option<PathBuf>,
        /// Only report symbols in this language.
        #[arg(long = "lang", alias = "language", value_name = "LANG")]
        languages: Vec<String>,
        /// Only report symbols under this path prefix.
        #[arg(long = "path", value_name = "PREFIX")]
        paths: Vec<PathBuf>,
        /// Only report symbols nothing outside their own file or package can see.
        #[arg(long)]
        internal: bool,
    },
    /// Remove unused imports and sort import blocks.
    Imports {
        /// File to organize.
        file: Option<PathBuf>,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Derive an OpenAPI document from a Next.js route tree.
    Openapi {
        /// Write the document here instead of to standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write YAML instead of JSON.
        #[arg(long)]
        yaml: bool,
    },
    /// Run a refactoring recipe: a file that says what to find, what to do to it, and what must
    /// be true afterwards.
    #[command(
        verbatim_doc_comment,
        about = "Run a refactoring recipe: find, do, expect"
    )]
    Recipe {
        #[command(subcommand)]
        command: Option<RecipeCommand>,
        /// The recipe file. Omit it with `--vocabulary`.
        file: Option<PathBuf>,
        /// Apply the changes instead of printing a diff.
        #[arg(long)]
        write: bool,
        /// Parse the file and print its plan, the steps, selectors and
        /// expectations, without running anything.
        #[arg(long)]
        explain: bool,
        /// Additional catalog directory for entry-point rules, as `fr unused` takes.
        #[arg(long)]
        catalogs: Vec<PathBuf>,
        /// Print what a recipe may say: the verbs, their argument forms, the
        /// predicates each kind of step takes, and the rewrites this build has.
        #[arg(long)]
        vocabulary: bool,
    },
    Spec {
        #[command(subcommand)]
        command: SpecCommand,
    },
    /// Rewrite a file as another language, beside the original.
    Translate {
        /// File to rewrite, or a directory to sweep file by file.
        file: PathBuf,
        /// Target language, `fastapi` for a Next.js API route, or `nextjs` for a
        /// FastAPI application.
        language: Option<String>,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
        /// Write the result here instead of beside the original.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Overwrite the destination when it already exists.
        #[arg(long)]
        force: bool,
    },
    /// Remove a feature flag and everything that only existed to serve it.
    RemoveFlag {
        /// The flag's name, or a position as `path:line:col` where more than one use
        /// shares it.
        flag: String,
        /// The value to assume it always had.
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        value: bool,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Apply a local transformation, or list the ones that apply.
    Rewrite {
        /// Position as `path:line:col`.
        target: String,
        /// Which transformation. Named from `Rewrite::ALL`, so a rewrite this
        /// build has cannot go unlisted here.
        #[arg(value_parser = rewrite_names())]
        rewrite: Option<String>,
        /// Apply the change instead of printing a diff.
        #[arg(long)]
        write: bool,
    },
    /// Rewrite every occurrence of a code shape.
    Restructure {
        /// The shape to match, e.g.
        pattern: String,
        /// What to replace it with, e.g.
        template: String,
        /// Language to rewrite.
        #[arg(long = "lang")]
        language: String,
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
        /// A values file passed to helm with -f (repeatable; later files win).
        #[arg(long = "values", short = 'f', value_name = "FILE")]
        values: Vec<PathBuf>,
        /// A helm --set assignment: a.b=c, a[0].b=c, or several comma-separated.
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Like --set, but helm keeps the value a string.
        #[arg(long = "set-string", value_name = "KEY=VALUE")]
        set_string: Vec<String>,
        /// A helm --set-file assignment, which takes its value from the file it names.
        #[arg(long = "set-file", value_name = "KEY=PATH")]
        set_file: Vec<String>,
        /// A helm --set-json assignment: the value is JSON, and may set a subtree.
        #[arg(long = "set-json", value_name = "KEY=JSON")]
        set_json: Vec<String>,
    },
    /// Trace configuration values into the code that reads them.
    Stitch {
        /// Only chains for this environment variable.
        #[arg(long)]
        env: Option<String>,
        /// Only variables nothing in the workspace reads.
        #[arg(long)]
        orphaned: bool,
        /// Trace the *files* configuration names instead: the script a CI step
        /// runs, the template a Terraform resource renders.
        #[arg(long)]
        files: bool,
        /// Trace the `--flags` scripts pass to the programs that declare them.
        #[arg(long)]
        flags: bool,
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
        /// Only this kind, e.g.
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

#[derive(Subcommand)]
enum RecipeCommand {
    #[command(about = "Canonicalize recipe layouts without changing what they mean.")]
    Fmt {
        #[arg(required = true, help = "Recipe files or directories to format")]
        paths: Vec<PathBuf>,
        #[arg(
            long,
            help = "Replace the recipe file instead of writing its canonical form to standard output"
        )]
        write: bool,
        #[arg(long, help = "Exit unsuccessfully when the recipe needs formatting")]
        check: bool,
    },
}

#[derive(Subcommand)]
enum SpecCommand {
    #[command(about = "Report stale Lean specification anchors and unproved obligations")]
    Check {
        #[arg(help = "Lean spec files or directories; defaults to kernels and specs")]
        paths: Vec<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

/// A completion script, written from the command tree this binary really has.
fn completion_script(shell: CompletionShell) -> String {
    use clap::CommandFactory;
    let command = Cli::command();
    let globals: Vec<String> = command
        .get_arguments()
        .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
        .collect();
    let globals = globals.join(" ");
    let subcommands: Vec<(String, String, String)> = command
        .get_subcommands()
        .map(|sub| {
            let flags: Vec<String> = sub
                .get_arguments()
                .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
                .collect();
            // Two of the three shells put the description inside quotes, and none
            // of them shows more than one line.
            let about = sub
                .get_about()
                .map(|a| a.to_string())
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or_default()
                .replace(['\'', '"', '`', '$'], "");
            (sub.get_name().to_string(), about, flags.join(" "))
        })
        .collect();
    let names: Vec<&str> = subcommands.iter().map(|(n, _, _)| n.as_str()).collect();
    let names = names.join(" ");

    let mut lines: Vec<String> = Vec::new();
    match shell {
        CompletionShell::Bash => {
            lines.push("_fr() {".into());
            lines.push("    local cur command i".into());
            lines.push("    cur=\"${COMP_WORDS[COMP_CWORD]}\"".into());
            lines.push("    command=\"\"".into());
            lines.push("    for ((i = 1; i < COMP_CWORD; i++)); do".into());
            lines.push("        case \"${COMP_WORDS[i]}\" in".into());
            lines.push("            -*) ;;".into());
            lines.push("            *) command=\"${COMP_WORDS[i]}\"; break ;;".into());
            lines.push("        esac".into());
            lines.push("    done".into());
            lines.push("    if [ -z \"$command\" ]; then".into());
            lines.push(format!(
                "        COMPREPLY=($(compgen -W \"{names} {globals}\" -- \"$cur\"))"
            ));
            lines.push("        return 0".into());
            lines.push("    fi".into());
            lines.push("    local options=\"\"".into());
            lines.push("    case \"$command\" in".into());
            for (name, _, flags) in &subcommands {
                lines.push(format!("        {name})"));
                lines.push(format!("            options=\"{flags}\""));
                lines.push("            ;;".into());
            }
            lines.push("    esac".into());
            lines.push("    if [[ \"$cur\" == -* ]]; then".into());
            lines.push(format!(
                "        COMPREPLY=($(compgen -W \"$options {globals}\" -- \"$cur\"))"
            ));
            lines.push("    else".into());
            lines.push("        COMPREPLY=($(compgen -f -- \"$cur\"))".into());
            lines.push("    fi".into());
            lines.push("}".into());
            lines.push("complete -F _fr fr".into());
        }
        CompletionShell::Zsh => {
            lines.push("#compdef fr".into());
            lines.push("_fr() {".into());
            lines.push("    local -a commands".into());
            lines.push("    commands=(".into());
            for (name, about, _) in &subcommands {
                lines.push(format!("        '{name}:{about}'"));
            }
            lines.push("    )".into());
            lines.push("    if (( CURRENT == 2 )); then".into());
            lines.push("        _describe -t commands 'fr command' commands".into());
            lines.push("    else".into());
            lines.push("        _files".into());
            lines.push("    fi".into());
            lines.push("}".into());
            lines.push("compdef _fr fr".into());
        }
        CompletionShell::Fish => {
            for flag in globals.split_whitespace() {
                lines.push(format!(
                    "complete -c fr -l {}",
                    flag.trim_start_matches('-')
                ));
            }
            for (name, about, flags) in &subcommands {
                lines.push(format!(
                    "complete -c fr -n __fish_use_subcommand -a {name} -d '{about}'"
                ));
                for flag in flags.split_whitespace() {
                    lines.push(format!(
                        "complete -c fr -n \"__fish_seen_subcommand_from {name}\" -l {}",
                        flag.trim_start_matches('-')
                    ));
                }
            }
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

pub fn run() -> Result<()> {
    let matches = <Cli as clap::CommandFactory>::command().get_matches();
    let mut cli = <Cli as clap::FromArgMatches>::from_arg_matches(&matches)
        .map_err(|e| Fault::invalid_input(e.to_string()))?;
    if matches.value_source("root") != Some(clap::parser::ValueSource::CommandLine) {
        if let Some(project) = enclosing_project(&cli.root) {
            eprintln!(
                "Reading the whole of {}, the project {} sits in. Pass `-C .` for \
                 this directory alone.",
                project.display(),
                cli.root.display()
            );
            cli.root = project;
            cli.root_inferred = true;
        }
    }
    let cli = cli;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let result = match crate::vfs::exists(&cli.root) {
        true => dispatch(&cli),
        false => Err(Fault::invalid_input(format!(
            "workspace root {} does not exist.",
            cli.root.display()
        ))),
    };
    let Err(error) = result else {
        return Ok(());
    };
    if cli.json {
        report_json_error(&error);
    }
    // The same prose, in the same shape, that returning the error from `main` printed.
    eprintln!("Error: {}", humanize_paths(&cli, format!("{error:?}")));
    std::process::exit(exit_code(&error));
}

/// Strip the workspace root out of error prose, so a human reads relative paths.
fn humanize_paths(cli: &Cli, prose: String) -> String {
    let root = workspace_root(cli);
    let prefix = format!("{}{}", root.display(), std::path::MAIN_SEPARATOR);
    prose.replace(&prefix, "")
}

/// The exit code a failure earns, mirroring the JSON error's `kind`.
fn exit_code(error: &anyhow::Error) -> i32 {
    if let Some(fault) = error.downcast_ref::<Fault>() {
        return match fault.kind {
            FaultKind::NotFound => 3,
            FaultKind::Ambiguous => 4,
            FaultKind::InvalidInput => 2,
        };
    }
    if error.chain().any(|c| c.is::<crate::refactor::Refusal>()) {
        return 5;
    }
    1
}

fn dispatch(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Capabilities {
            capability,
            language,
            markdown,
        } => cmd_capabilities(cli, capability.as_deref(), language.as_deref(), *markdown),
        Command::Completions { shell } => {
            print!("{}", completion_script(*shell));
            Ok(())
        }
        Command::Cache { clear } => cmd_cache(cli, *clear),
        Command::Scan { languages } => cmd_scan(cli, languages),
        Command::Parse { languages, stats } => cmd_parse(cli, languages, *stats),
        Command::Symbols {
            languages,
            name,
            kind,
            stats,
            paths,
        } => cmd_symbols(
            cli,
            languages,
            name.as_deref(),
            kind.as_deref(),
            *stats,
            paths,
        ),
        Command::Def { target, first } => cmd_def(cli, target, *first),
        Command::Type { target } => cmd_type(cli, target),
        Command::Implementations { target } => cmd_implementations(cli, target),
        Command::Usages {
            target,
            include_unresolved,
        } => cmd_usages(cli, target, *include_unresolved),
        Command::Refs {
            target,
            include_unresolved,
        } => cmd_refs(cli, target, *include_unresolved),
        Command::Rename {
            target,
            new_name,
            write,
        } => cmd_rename(cli, target, new_name, *write),
        Command::Callers { target, depth } => cmd_trace(cli, target, *depth, Direction2::Callers),
        Command::Callees { target, depth } => cmd_trace(cli, target, *depth, Direction2::Callees),
        Command::Flow {
            direction,
            target,
            depth,
            values,
            set,
            set_string,
            set_file,
            set_json,
        } => {
            let inputs = crate::analysis::provenance::ValuesInputs::parse(
                values,
                crate::analysis::provenance::SetFlags {
                    sets: set,
                    strings: set_string,
                    files: set_file,
                    jsons: set_json,
                },
            )?;
            cmd_flow(cli, direction, target, *depth, &inputs)
        }
        Command::Extract {
            range,
            name,
            function,
            all,
            write,
        } => cmd_extract(
            cli,
            range,
            name,
            match *function {
                true => Extract::Function,
                false => Extract::Variable,
            },
            match *all {
                true => Occurrences::All,
                false => Occurrences::First,
            },
            *write,
        ),
        Command::Inline {
            target,
            call,
            write,
        } => cmd_inline(
            cli,
            target,
            match *call {
                true => Inline::Call,
                false => Inline::Variable,
            },
            *write,
        ),
        Command::Openapi { out, yaml } => cmd_openapi(cli, out.as_deref(), *yaml),
        Command::Recipe {
            command,
            file,
            write,
            explain,
            catalogs,
            vocabulary,
        } => match command {
            Some(RecipeCommand::Fmt {
                paths,
                write,
                check,
            }) => cmd_recipe_fmt(cli, paths, *write, *check),
            None => cmd_recipe(
                cli,
                file.as_deref(),
                *write,
                *explain,
                catalogs,
                *vocabulary,
            ),
        },
        Command::Spec { command } => match command {
            SpecCommand::Check { paths } => cmd_spec_check(cli, paths),
        },
        Command::RemoveFlag { flag, value, write } => {
            cmd_remove_flag(cli, flag, FlagValue(*value), *write)
        }
        Command::Rewrite {
            target,
            rewrite,
            write,
        } => cmd_rewrite(cli, target, rewrite.as_deref(), *write),
        Command::Restructure {
            pattern,
            template,
            language,
            write,
        } => cmd_restructure(cli, pattern, template, language, *write),
        Command::Delete { target, write } => cmd_delete(cli, target, *write),
        Command::Duplicates {
            min_tokens,
            exact,
            languages,
            paths,
        } => cmd_duplicates(cli, *min_tokens, *exact, languages, paths),
        Command::Unused {
            catalogs,
            languages,
            paths,
            internal,
        } => cmd_unused(cli, catalogs.as_deref(), languages, paths, *internal),
        Command::Imports { file, write } => cmd_imports(cli, file.as_deref(), *write),
        Command::Translate {
            file,
            language,
            write,
            out,
            force,
        } => cmd_translate(
            cli,
            file,
            language.as_deref(),
            *write,
            out.as_deref(),
            *force,
        ),
        Command::Move {
            target,
            destination,
            write,
        } => cmd_move(cli, target, destination, *write),
        Command::Signature {
            target,
            change,
            write,
        } => cmd_signature(cli, target, change, *write),
        Command::Stitch {
            env,
            orphaned,
            files,
            flags,
        } => cmd_stitch(cli, env.as_deref(), *orphaned, *files, *flags),
        Command::Impact {
            target,
            caller_depth,
        } => cmd_impact(cli, target, *caller_depth),
        Command::Graph { dot } => cmd_graph(cli, *dot),
        Command::Entrypoints {
            kind,
            catalogs,
            unreachable,
        } => cmd_entrypoints(cli, kind.as_deref(), catalogs.as_deref(), *unreachable),
    }
}

fn cmd_spec_check(cli: &Cli, paths: &[PathBuf]) -> Result<()> {
    let root = workspace_root(cli);
    let report = crate::spec::check(&root, paths, !cli.no_ignore)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for anchor in &report.anchors {
            let target = format!("{}::{}", anchor.source.display(), anchor.symbol);
            match anchor.status {
                crate::spec::Status::Fresh => {
                    println!("fresh {}:{} {}", anchor.spec.display(), anchor.line, target)
                }
                crate::spec::Status::Stale => println!(
                    "stale {}:{} {} (expected {}, found {})",
                    anchor.spec.display(),
                    anchor.line,
                    target,
                    anchor.expected,
                    anchor.actual.as_deref().unwrap_or("nothing")
                ),
                crate::spec::Status::Missing => println!(
                    "missing {}:{} {} ({})",
                    anchor.spec.display(),
                    anchor.line,
                    target,
                    anchor.detail.as_deref().unwrap_or("no declaration found")
                ),
            }
        }
        println!(
            "{} fresh, {} stale, {} missing; {} unproved obligation(s).",
            report.fresh(),
            report.stale(),
            report.missing(),
            report.obligations
        );
    }
    if report.ok() {
        Ok(())
    } else {
        anyhow::bail!(
            "spec check found {} stale and {} missing anchor(s)",
            report.stale(),
            report.missing()
        )
    }
}

fn cmd_trace(cli: &Cli, target: &str, depth: usize, direction: Direction2) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    if !symbol.kind.is_callable() {
        anyhow::bail!(
            "'{}' is {}, not a function or method. It has no call edges",
            symbol.name,
            symbol.kind.with_article()
        );
    }
    // The matrix says `n/a` for this language's call graph, and the caller pointed at one
    // symbol, so they are owed an answer about it.
    if let crate::capabilities::Support::NotApplicable { because } =
        crate::capabilities::support(crate::capabilities::Capability::CallGraph, symbol.language)
    {
        return Err(crate::refactor::Refusal::Unsupported {
            operation: match direction {
                Direction2::Callers => "showing what calls a function".into(),
                Direction2::Callees => "showing what a function calls".into(),
            },
            language: symbol.language,
            because,
        }
        .into());
    }

    let graph = build_call_graph(cli, &index);
    let trace = graph.trace(symbol.id, direction, depth);

    if cli.json {
        // Name and depth alone flattened the tree, and no caller could rebuild the walk from
        // two branches at the same depth.
        let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
        let mut line_of = |s: &Symbol| {
            let source = sources
                .entry(s.file.clone())
                .or_insert_with(|| crate::vfs::read_to_string(&s.file).unwrap_or_default());
            LineIndex::new(source)
                .line_col(s.name_span.start, source)
                .line
        };
        let nodes: Vec<_> = trace
            .nodes
            .iter()
            .map(|n| {
                let s = index.symbol(n.symbol);
                let parent = n
                    .caller
                    .and_then(|(id, _)| index.symbol(id))
                    .map(|p| serde_json::json!({ "name": p.qualified_name(), "file": p.file }));
                serde_json::json!({
                    "name": s.map(|s| s.qualified_name()),
                    "file": s.map(|s| s.file.clone()),
                    "line": s.map(&mut line_of),
                    "depth": n.depth,
                    "parent": parent,
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

/// A unified diff whose headers `git apply -p1` accepts.
fn workspace_diff(cli: &Cli, outcome: &crate::edit::FileOutcome) -> String {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let shown = outcome.path.strip_prefix(&root).unwrap_or(&outcome.path);
    crate::edit::unified_diff(
        &outcome.original,
        &outcome.updated,
        &shown.display().to_string(),
    )
}

/// The project `start` sits inside, when that is somewhere above `start`.
fn enclosing_project(start: &std::path::Path) -> Option<PathBuf> {
    const MARKERS: &[&str] = &[
        ".git",
        "Cargo.toml",
        "go.mod",
        "package.json",
        "pyproject.toml",
        "setup.py",
        "build.zig",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
    ];
    let from = start.canonicalize().ok()?;
    if !from.is_dir() {
        return None;
    }
    let found = from.ancestors().find(|dir| {
        MARKERS
            .iter()
            .any(|marker| crate::vfs::exists(dir.join(marker).as_path()))
    })?;
    // Already at the project root: the default is right and needs no note.
    (found != from).then(|| found.to_path_buf())
}

/// The canonical workspace root, the spelling the index writes into every path.
fn workspace_root(cli: &Cli) -> PathBuf {
    cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone())
}

/// A path spelled the way human listings print it: relative to the workspace root.
fn shown_path(root: &std::path::Path, path: &std::path::Path) -> String {
    match path.strip_prefix(root) {
        Ok(rest) if !rest.as_os_str().is_empty() => rest.display().to_string(),
        // `-C` can name a single file, and then the root *is* the file.
        Ok(_) => match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => path.display().to_string(),
        },
        Err(_) => path.display().to_string(),
    }
}

/// The files the scan never read, as data for a JSON report.
fn skipped_files_json(index: &Index) -> Vec<serde_json::Value> {
    index
        .skipped
        .iter()
        .map(|(path, reason)| serde_json::json!({ "file": path, "reason": reason }))
        .collect()
}

/// The files the grammar read only in part, as data for a JSON report.
fn unparsed_files_json(index: &Index) -> Vec<serde_json::Value> {
    index
        .unparsed()
        .map(|path| serde_json::json!({ "file": path, "reason": "file has syntax errors" }))
        .collect()
}

/// Warn, on stderr, about the files this answer saw partially or not at all.
fn warn_partial_index(cli: &Cli, index: &Index) {
    if cli.json {
        return;
    }
    let root = workspace_root(cli);
    if !index.skipped.is_empty() {
        eprintln!("Warning: {} file(s) were not read.", index.skipped.len());
        eprintln!("The answer cannot see anything in them:");
        for (path, reason) in index.skipped.iter().take(10) {
            eprintln!("  {} ({reason})", shown_path(&root, path));
        }
        if index.skipped.len() > 10 {
            eprintln!("  … and {} more", index.skipped.len() - 10);
        }
        eprintln!("Raise --max-file-size to include a file skipped for its size.");
    }
    let unparsed: Vec<_> = index.unparsed().collect();
    if unparsed.is_empty() {
        return;
    }
    eprintln!("Warning: {} file(s) did not parse in full.", unparsed.len());
    eprintln!("The answer sees only the part the grammar could read:");
    for path in unparsed.iter().take(10) {
        eprintln!("  {}", shown_path(&root, path));
    }
    if unparsed.len() > 10 {
        eprintln!("  … and {} more", unparsed.len() - 10);
    }
    eprintln!("Run `fr parse` for the positions the grammar stopped at.");
}

/// Refuse a plan whose files changed after the index read them.
fn refuse_stale_plan(index: &Index, edits: &crate::edit::EditSet) -> Result<()> {
    for path in edits.paths() {
        let Some(recorded) = index.content_hash(path) else {
            // No index holds a file this plan creates. The commit path re-reads and
            // guards it.
            continue;
        };
        let current = match crate::vfs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!(
                    "{} left the tree after the index read it. Nothing written. Re-run \
                     the command against the current tree.",
                    path.display()
                );
            }
            Err(e) => {
                return Err(anyhow::Error::from(e))
                    .with_context(|| format!("re-reading {}", path.display()))
            }
        };
        if crate::index::content_hash_of(&current) != recorded {
            anyhow::bail!(
                "{} changed after the plan read it. Nothing written. Re-run \
                 the command against the current text.",
                path.display()
            );
        }
    }
    Ok(())
}

/// Render a plan's diff, report what it did, and optionally commit it.
fn present(
    cli: &Cli,
    index: Option<&Index>,
    edits: &crate::edit::EditSet,
    summary: &str,
    write: bool,
) -> Result<()> {
    present_with(cli, index, edits, summary, write, |_| {})
}

/// [`present`], with fields only one command has to add to the JSON report.
fn present_with(
    cli: &Cli,
    index: Option<&Index>,
    edits: &crate::edit::EditSet,
    summary: &str,
    write: bool,
    decorate: impl FnOnce(&mut serde_json::Value),
) -> Result<()> {
    if let Some(index) = index {
        refuse_stale_plan(index, edits)?;
    }
    let outcomes = crate::edit::plan(edits, crate::edit::Validation::ReparseStrict)?;

    if cli.json {
        let changes: Vec<_> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "file": o.path,
                    // The same path under the key older scripts read.
                    "path": o.path,
                    "diff": workspace_diff(cli, o),
                })
            })
            .collect();
        let mut report = serde_json::json!({
            "summary": summary,
            "files_changed": outcomes.len(),
            "applied": write,
            "changes": changes,
            "skipped_files": index.map(skipped_files_json).unwrap_or_default(),
            "unparsed_files": index.map(unparsed_files_json).unwrap_or_default(),
        });
        decorate(&mut report);
        println!("{}", serde_json::to_string_pretty(&report)?);
        if write {
            crate::edit::commit(&outcomes)?;
        }
        return Ok(());
    }

    for outcome in &outcomes {
        print!("{}", workspace_diff(cli, outcome));
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

/// One translated file's JSON report, with the fidelity keys the directory sweep emits.
fn present_translation(
    cli: &Cli,
    edits: &crate::edit::EditSet,
    fidelity: &crate::transpile::Fidelity,
    summary: &str,
    write: bool,
    decorate: impl FnOnce(&mut serde_json::Value),
) -> Result<()> {
    let outcomes = crate::edit::plan(edits, crate::edit::Validation::ReparseStrict)?;
    let changes: Vec<_> = outcomes
        .iter()
        .map(|o| {
            serde_json::json!({
                "file": o.path,
                "path": o.path,
                "diff": workspace_diff(cli, o),
            })
        })
        .collect();
    let mut report = serde_json::json!({
        "summary": summary,
        "functions": fidelity.functions,
        "records": fidelity.records,
        "constants": fidelity.constants,
        "newtypes": fidelity.newtypes,
        "sums": fidelity.sums,
        "signatures_complete": fidelity.signatures_complete,
        "signatures_with_foreign_types": fidelity.signatures_with_foreign_types,
        "carried_verbatim": fidelity.carried_verbatim,
        "notes": fidelity.notes,
        "files_changed": outcomes.len(),
        "applied": write,
        "changes": changes,
    });
    decorate(&mut report);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if write {
        crate::edit::commit(&outcomes)?;
    }
    Ok(())
}

/// What `fr extract` pulls out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Extract {
    Variable,
    Function,
}

/// How many occurrences `fr extract` replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Occurrences {
    First,
    All,
}

/// What `fr inline` replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inline {
    Variable,
    Call,
}

/// The value a removed flag is fixed at, distinct from the `write` beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlagValue(bool);

fn cmd_extract(
    cli: &Cli,
    range: &str,
    name: &str,
    extract: Extract,
    occurrences: Occurrences,
    write: bool,
) -> Result<()> {
    // A malformed or inverted range is a command-line fault, and the shared parser returns an
    // untyped error because a recipe reads the same spec.
    let (path, start, end) =
        crate::span::parse_range(range).map_err(|e| Fault::invalid_input(e.to_string()))?;
    let path = workspace_path(cli, &path)?;
    let source =
        crate::vfs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let span = crate::span::Span::new(
        offset_at(&source, start.line, start.col, &path)?,
        offset_at(&source, end.line, end.col, &path)?,
    );

    let index = build_index(cli, &[])?;

    if extract == Extract::Function {
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
        return present(cli, Some(&index), &plan.edits, &summary, write);
    }

    let plan = crate::refactor::extract::variable(
        &index,
        &path,
        span,
        name,
        occurrences == Occurrences::All,
    )?;
    let summary = format!(
        "extracted `{}` into {} ({} occurrence(s) replaced)",
        plan.expression.trim(),
        plan.name,
        plan.occurrences
    );
    present(cli, Some(&index), &plan.edits, &summary, write)
}

fn cmd_inline(cli: &Cli, target: &str, inline: Inline, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;

    if inline == Inline::Call {
        let pos = parse_position(target).ok_or_else(|| {
            Fault::invalid_input(
                "inlining a call needs a position: path:line:col of the call".to_string(),
            )
        })?;
        refuse_zero_column(&pos)?;
        let path = workspace_path(cli, &pos.path)?;
        let source = crate::vfs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let offset = offset_at(&source, pos.line, pos.col, &path)?;

        let plan = crate::refactor::inline::call(&index, &path, offset)?;
        let summary = format!(
            "inlined the call to {} as `{}`",
            plan.function, plan.expansion
        );
        return present(cli, Some(&index), &plan.edits, &summary, write);
    }

    let symbol = resolve_target(cli, &index, target)?;
    let plan = crate::refactor::inline::variable(&index, symbol.id)?;
    let summary = format!(
        "inlined `{}` into {} use site(s)",
        plan.name, plan.use_sites
    );
    present(cli, Some(&index), &plan.edits, &summary, write)
}

fn cmd_signature(cli: &Cli, target: &str, change_spec: &str, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let change = crate::refactor::signature::Change::parse(change_spec)?;
    let plan = crate::refactor::signature::change(&index, symbol.id, change)?;
    let summary = crate::refactor::signature::describe(&index, &plan);
    present(cli, Some(&index), &plan.edits, &summary, write)
}

/// The destination file, spelled the way the index spells its paths.
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
             give a path inside an existing directory.",
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
    present(cli, Some(&index), &plan.edits, &summary, write)
}

fn cmd_delete(cli: &Cli, target: &str, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let plan = crate::refactor::delete::plan(&index, symbol.id)?;

    if !plan.warnings.is_empty() && !cli.json {
        let root = workspace_root(cli);
        println!("Review these before committing:");
        for w in plan.warnings.iter().take(20) {
            println!(
                "  {}:{}:{}  {}",
                shown_path(&root, &w.file),
                w.line,
                w.col,
                w.detail
            );
        }
        if plan.warnings.len() > 20 {
            println!("  … and {} more", plan.warnings.len() - 20);
        }
        println!();
    }

    let summary = format!("deleted {} ({} definition site(s))", plan.name, plan.sites);
    present(cli, Some(&index), &plan.edits, &summary, write)
}

/// A path the caller typed, spelled the way the index spells its paths.
fn workspace_path(cli: &Cli, path: &std::path::Path) -> Result<PathBuf> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    // An absolute path is already where it says.
    let as_typed = path.is_absolute() || (cli.root_inferred && crate::vfs::exists(path));
    let joined = match as_typed {
        true => path.to_path_buf(),
        false => root.join(path),
    };
    // A path that fails to resolve names no target, and the exit code says so.
    joined.canonicalize().map_err(|e| {
        if path.is_absolute() || root == std::path::Path::new(".") {
            Fault::not_found(format!("{}: {e}", path.display()))
        } else {
            Fault::not_found(format!(
                "{} does not exist in {}. Every path resolves against the workspace \
                 root, which -C set to that. ({e})",
                path.display(),
                root.display()
            ))
        }
    })
}

/// The byte offset of a 1-based position, refused as not-found when the file ends before it.
fn offset_at(source: &str, line: usize, col: usize, path: &std::path::Path) -> Result<usize> {
    LineIndex::new(source)
        .offset(LineCol { line, col }, source)
        .ok_or_else(|| Fault::not_found(format!("{line}:{col} is outside {}", path.display())))
}

/// Language names from the command line, refused and not ignored.
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
    min_tokens: Option<usize>,
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

    let root = workspace_root(cli);
    if classes.is_empty() {
        match min_tokens {
            Some(stated) => println!(
                "No duplication of {stated} tokens or more{}.",
                if exact { " (exact)" } else { "" }
            ),
            None => println!(
                "No duplication of 60 tokens or more in code, or 30 in markup and \
                 configuration{}. Pass --min-tokens to look for smaller copies.",
                if exact { " (exact)" } else { "" }
            ),
        }
    }
    for class in &classes {
        println!(
            "{} copies, {} tokens each ({} redundant): {}",
            class.instances.len(),
            class.tokens,
            class.redundant_tokens(),
            class.language
        );
        for instance in &class.instances {
            println!(
                "  {}:{}-{}",
                shown_path(&root, &instance.file),
                instance.start_line,
                instance.end_line
            );
        }
    }

    if !classes.is_empty() {
        let redundant: usize = classes.iter().map(|c| c.redundant_tokens()).sum();
        // The threshold belongs here as much as it does in the empty case.
        let floor = match min_tokens {
            Some(stated) => format!("{stated} tokens or more"),
            None => "60 tokens or more in code, 30 in markup and configuration".to_string(),
        };
        println!(
            "\n{} duplicated block(s) of {floor}, {redundant} redundant token(s).",
            classes.len()
        );
        println!(
            "This compares structure, not text, so a copy with renamed variables still \n\
             matches; pass --exact to require the names too. Each duplication shows \n\
             its largest block alone. The statements inside it repeat as well, and \n\
             saying so again would bury the finding. Smaller copies exist in most \n\
             codebases and fall below the line --min-tokens draws."
        );
    }

    let skipped = duplicates::unparsed(&index, &options);
    if !skipped.is_empty() {
        println!(
            "\n{} file(s) do not parse, so this skips them and says nothing about \n\
             duplication in them:",
            skipped.len()
        );
        for path in skipped.iter().take(10) {
            println!("  {}", shown_path(&root, path));
        }
        if skipped.len() > 10 {
            println!("  … and {} more", skipped.len() - 10);
        }
    }
    Ok(())
}

/// The caveat both renderings of `fr unused` carry, one copy so they cannot drift.
const UNUSED_CAVEAT: &str =
    "Reachability follows resolved call edges plus class-hierarchy dispatch \n\
     candidates. So a method reached only through a trait object, an interface \n\
     value or a base class is no longer listed. A function held in a map or a \n\
     struct field and called through it, and a name assembled at runtime, still \n\
     can be. This leaves off, on purpose, every symbol whose name appears in a \n\
     string literal. It leaves off a name beginning with an underscore, which says \n\
     the author meant it to go unused. It leaves off a file a tool writes, such as \n\
     a lock file.";

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
    let graph = build_call_graph(cli, &index);
    let unused = crate::refactor::delete::find_unused_with_graph(&index, &entrypoints, &graph);

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

    // The position, because the next command a reader runs is `fr delete`.
    let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut locate = |symbol: &crate::model::Symbol| {
        let source = sources
            .entry(symbol.file.clone())
            .or_insert_with(|| crate::vfs::read_to_string(&symbol.file).unwrap_or_default());
        LineIndex::new(source).line_col(symbol.name_span.start, source)
    };

    if cli.json {
        let payload: Vec<_> = unused
            .iter()
            .filter_map(|id| index.symbol(*id))
            .map(|s| {
                let at = locate(s);
                serde_json::json!({
                    "name": s.qualified_name(),
                    "kind": s.kind.as_str(),
                    "file": s.file,
                    "line": at.line,
                    "col": at.col,
                    "language": s.language.name(),
                    "exported": s.exported,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "unused": payload,
                "caveat": UNUSED_CAVEAT.replace('\n', ""),
                "skipped_files": skipped_files_json(&index),
                "unparsed_files": unparsed_files_json(&index),
            }))?
        );
        return Ok(());
    }

    let root = workspace_root(cli);
    let mut exported_count = 0usize;
    for symbol in unused.iter().filter_map(|id| index.symbol(*id)) {
        if symbol.exported {
            exported_count += 1;
        }
        let at = locate(symbol);
        println!(
            "{:<14} {:<34} {:<9} {}:{at}",
            symbol.kind.as_str(),
            symbol.qualified_name(),
            if symbol.exported { "exported" } else { "" },
            shown_path(&root, &symbol.file)
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

    // What a long answer is mostly made of.
    if unused.len() >= 50 {
        let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        let mut by_file: BTreeMap<&std::path::Path, usize> = BTreeMap::new();
        for symbol in unused.iter().filter_map(|id| index.symbol(*id)) {
            *by_kind.entry(symbol.kind.as_str()).or_default() += 1;
            *by_file.entry(symbol.file.as_path()).or_default() += 1;
        }
        let mut kinds: Vec<_> = by_kind.into_iter().collect();
        kinds.sort_by_key(|(name, count)| (std::cmp::Reverse(*count), *name));
        let shown: Vec<String> = kinds
            .iter()
            .take(5)
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        let rest = match kinds.len() > shown.len() {
            true => format!(", and {} other kind(s)", kinds.len() - shown.len()),
            false => String::new(),
        };
        println!("  {}{rest}", shown.join(", "));
        if let Some((path, count)) = by_file.iter().max_by_key(|(path, count)| (**count, *path)) {
            // Only when one file dominates: otherwise the answer is spread out and
            // naming its largest single file says nothing.
            if *count * 2 > unused.len() {
                println!("  {count} of them in {}", shown_path(&root, path));
            }
        }
    }
    println!("{UNUSED_CAVEAT}");
    if exported_count > 0 && !internal_only {
        let verb = match exported_count {
            1 => "is",
            _ => "are",
        };
        println!(
            "\n{exported_count} of these {verb} exported. In a library that is the public \n\
             API, which nothing in this repository can be expected to call. Pass \n\
             --internal to list only what is definitely dead here."
        );
    }
    Ok(())
}

fn cmd_imports(cli: &Cli, file: Option<&std::path::Path>, write: bool) -> Result<()> {
    let index = build_index(cli, &[])?;

    let Some(file) = file else {
        return cmd_imports_workspace(cli, &index, write);
    };

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
            "\nThe name decides liveness, so an import held only for a trait, a \n\
             registration side effect or a doc comment would look unused. Check these.\n"
        );
    }

    // Every import nothing names and the planner kept anyway, with the reason it worked out.
    if !cli.json && !plan.warnings.is_empty() {
        println!("Kept {} import(s) nothing names:", plan.warnings.len());
        for warning in &plan.warnings {
            println!("  line {}: {}", warning.line, warning.detail);
        }
        println!();
    }

    let summary = format!(
        "{}: removed {} import(s), reordered {} block(s)",
        plan.file.display(),
        plan.removed.len(),
        plan.sorted_blocks
    );
    let kept = kept_imports_json(&plan.warnings);
    present_with(cli, Some(&index), &plan.edits, &summary, write, |report| {
        report["kept_imports"] = kept;
    })
}

/// The imports the planner held back, as data, each with the reason it printed.
fn kept_imports_json(warnings: &[crate::refactor::Warning]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = warnings
        .iter()
        .map(|warning| {
            serde_json::json!({
                "file": warning.file,
                "line": warning.line,
                "col": warning.col,
                "reason": warning.detail,
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

/// Every file the index holds, one pass, one atomic apply.
fn cmd_imports_workspace(cli: &Cli, index: &crate::index::Index, write: bool) -> Result<()> {
    let mut edits = crate::edit::EditSet::new();
    let mut touched = 0usize;
    let mut removed = 0usize;
    let mut reordered = 0usize;
    let mut skipped: std::collections::BTreeMap<String, usize> = Default::default();
    let mut kept: Vec<crate::refactor::Warning> = Vec::new();

    let files: Vec<std::path::PathBuf> = index.files().map(|(path, _)| path.clone()).collect();
    for path in files {
        match crate::refactor::imports::plan(index, &path) {
            Ok(plan) => {
                // Counted before the loop drops the files with nothing to do.
                kept.extend(plan.warnings.iter().cloned());
                if plan.edits.is_empty() {
                    continue;
                }
                touched += 1;
                removed += plan.removed.len();
                reordered += plan.sorted_blocks;
                if !cli.json {
                    println!(
                        "{}: removing {} import(s), reordering {} block(s).",
                        plan.file.display(),
                        plan.removed.len(),
                        plan.sorted_blocks
                    );
                }
                edits.extend(plan.edits);
            }
            Err(error) => {
                *skipped.entry(error.to_string()).or_default() += 1;
            }
        }
    }

    if !cli.json && !skipped.is_empty() {
        println!("\nSkipped, with the tool's reasons:");
        for (reason, count) in &skipped {
            let first = reason.lines().next().unwrap_or(reason);
            println!("  {count} file(s): {first}");
        }
    }
    // A sweep of a workspace would drown in one line per held import.
    if !cli.json && !kept.is_empty() {
        println!(
            "\n{} import(s) that nothing names stay. `fr imports <file>` says why.",
            kept.len()
        );
    }
    if !cli.json {
        println!();
    }

    let summary = format!(
        "workspace: {touched} file(s) changed, {removed} import(s) removed, \
         {reordered} block(s) reordered."
    );
    let kept = kept_imports_json(&kept);
    present_with(cli, Some(index), &edits, &summary, write, |report| {
        report["kept_imports"] = kept;
    })
}

fn cmd_translate(
    cli: &Cli,
    file: &std::path::Path,
    language: Option<&str>,
    write: bool,
    out: Option<&std::path::Path>,
    force: bool,
) -> Result<()> {
    let path = workspace_path(cli, file)?;
    if path.is_dir() {
        return cmd_translate_directory(cli, &path, language, write, force);
    }
    // The destination may not exist yet, so it cannot go through `workspace_path`, which
    // canonicalizes.
    let out = out.map(|o| {
        if o.is_absolute() {
            o.to_path_buf()
        } else {
            cli.root
                .canonicalize()
                .unwrap_or_else(|_| cli.root.clone())
                .join(o)
        }
    });
    let out = out.as_deref();
    let from = crate::lang::detect(&path)
        .ok_or_else(|| anyhow::anyhow!("{} is not a language this build reads", path.display()))?;

    let Some(language) = language else {
        // No target named: say what this file could be, and stop.
        let options = crate::translate::options_for(&path);
        let route = crate::transpile::nextjs::is_api_route(&path)
            .then(|| crate::transpile::nextjs::plan(&path).ok())
            .flatten();
        if cli.json {
            // The listing as data, with each draft's fidelity in the shape the directory sweep
            // reports.
            let mut listed: Vec<serde_json::Value> = Vec::new();
            if let Some(plan) = &route {
                listed.push(serde_json::json!({
                    "target": "fastapi",
                    "destination": plan.destination,
                    "route": plan.route,
                    "methods": plan.methods,
                    "blocked": serde_json::Value::Null,
                    "same_bytes": false,
                    "fidelity": serde_json::Value::Null,
                }));
            }
            for option in &options {
                listed.push(serde_json::json!({
                    "target": option.target.name(),
                    "destination": option.destination,
                    "blocked": option.blocked,
                    "same_bytes": option.blocked.is_none() && option.fidelity.is_none(),
                    "fidelity": option.fidelity,
                }));
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "file": path,
                    "language": from.name(),
                    "options": listed,
                }))?
            );
            return Ok(());
        }
        if options.is_empty() && route.is_none() {
            println!(
                "{} is {from}, and no language can hold it.\n\n{}",
                path.display(),
                crate::translate::why_nothing(from)
            );
            return Ok(());
        }
        println!("{} is {from}. These languages can hold it:", path.display());
        if let Some(plan) = route {
            println!(
                "  {:<10} -> {} (route {}, {})",
                "fastapi",
                plan.destination.display(),
                plan.route,
                plan.methods.join(", ")
            );
        }
        for option in &options {
            let target = option.target;
            if let Some(reason) = &option.blocked {
                println!(
                    "  {target:<10} -> {} (blocked: {reason})",
                    option.destination.display()
                );
                continue;
            }
            match &option.fidelity {
                None => println!(
                    "  {target:<10} -> {} (same bytes).",
                    option.destination.display()
                ),
                Some(f) => println!(
                    "  {target:<10} -> {} (a draft: {}/{} signatures complete, {} \
                     construct(s) carried over).",
                    option.destination.display(),
                    f.signatures_complete,
                    f.functions,
                    f.carried_verbatim
                ),
            }
        }
        return Ok(());
    };

    // `fastapi` names a framework and not a language.
    let scaffolds = language.eq_ignore_ascii_case("fastapi")
        || language.eq_ignore_ascii_case("nextjs")
        || language.eq_ignore_ascii_case("next.js");
    if scaffolds && crate::transpile::scaffold::is_openapi_document(&path) {
        let target = match language.eq_ignore_ascii_case("fastapi") {
            true => crate::transpile::scaffold::Target::FastApi,
            false => crate::transpile::scaffold::Target::NextJs,
        };
        return cmd_scaffold(cli, &path, target, write, out, force);
    }
    if language.eq_ignore_ascii_case("fastapi") {
        return cmd_translate_fastapi(cli, &path, write, out, force);
    }
    // The other direction, which is one file to a *tree*.
    if language.eq_ignore_ascii_case("nextjs") || language.eq_ignore_ascii_case("next.js") {
        return cmd_translate_nextjs(cli, &path, write, out, force);
    }

    let to = crate::lang::Language::from_name(language)
        .ok_or_else(|| anyhow::anyhow!("unknown language '{language}'"))?;

    // Asking for the language a file already is usually means the reader wanted the
    // listing, so the refusal says how to get it.
    if to == from {
        anyhow::bail!(
            "{} is already {to}. Run 'fr translate {}' for the languages that can \
             hold it.",
            path.display(),
            file.display()
        );
    }

    if crate::translate::targets(from).contains(&to) {
        let plan = crate::translate::plan_to(&path, to, out, force)?;
        let summary = format!(
            "{} written as {} ({} -> {})",
            plan.source.display(),
            plan.destination.display(),
            plan.from,
            plan.to
        );
        return present(cli, None, &plan.edits, &summary, write);
    }

    let plan = crate::transpile::plan_to(&path, to, out, force)?;
    if cli.json {
        let summary = format!(
            "{} translated to {} ({} -> {})",
            plan.source.display(),
            plan.destination.display(),
            plan.from,
            plan.to
        );
        return present_translation(
            cli,
            &plan.edits,
            &plan.fidelity,
            &summary,
            write,
            |report| {
                report["target"] = serde_json::json!(plan.to.name());
                report["translated"] = serde_json::json!([plan.source]);
            },
        );
    }
    {
        let f = &plan.fidelity;
        let distinct = match f.newtypes {
            0 => String::new(),
            n => format!(", {n} distinct type(s)"),
        };
        let choices = match f.sums {
            0 => String::new(),
            n => format!(", {n} choice type(s)"),
        };
        println!(
            "{} -> {} ({} function(s), {} record(s), {} constant(s){distinct}{choices}).",
            plan.from, plan.to, f.functions, f.records, f.constants
        );
        println!(
            "  signatures: {} complete, {} mentioning a type this tool does not know",
            f.signatures_complete, f.signatures_with_foreign_types
        );
        // Not every note is about a carried construct.
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
    present(cli, None, &plan.edits, &summary, write)
}

/// Translate everything under a directory into one language, atomically.
fn cmd_translate_directory(
    cli: &Cli,
    dir: &std::path::Path,
    language: Option<&str>,
    write: bool,
    force: bool,
) -> Result<()> {
    let Some(language) = language else {
        anyhow::bail!(
            "{} is a directory; name the target language to translate everything under it.",
            dir.display()
        );
    };
    if language.eq_ignore_ascii_case("fastapi") {
        anyhow::bail!(
            "fastapi reads a route file's path as well as its text, so routes translate \
             one at a time."
        );
    }
    let to = crate::lang::Language::from_name(language)
        .ok_or_else(|| anyhow::anyhow!("unknown language '{language}'"))?;

    let scanned = scan(dir, &scan_options(cli, &[])?)?;

    // Read the whole sweep before writing any of it.
    let mut modules: BTreeMap<PathBuf, crate::transpile::Module> = BTreeMap::new();
    for file in &scanned.files {
        if file.language == to || !crate::transpile::can_be_read(file.language) {
            continue;
        }
        // A file that does not read is not lost here.
        if let Ok(module) = crate::transpile::read_file(&file.path) {
            modules.insert(file.path.clone(), module);
        }
    }
    let mut context = crate::transpile::Module::default();
    for module in modules.values() {
        context.items.extend(module.items.iter().cloned());
    }

    let mut edits = crate::edit::EditSet::new();
    let mut fidelity = crate::transpile::Fidelity::default();
    let mut translated: Vec<PathBuf> = Vec::new();
    let mut already_target = 0usize;
    let mut unreadable: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut occupied: Vec<PathBuf> = Vec::new();
    let mut failed: Vec<(PathBuf, String)> = Vec::new();
    let mut claimed: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();

    for file in &scanned.files {
        if file.language == to {
            already_target += 1;
            continue;
        }
        if !crate::transpile::can_be_read(file.language) {
            *unreadable.entry(file.language.name()).or_default() += 1;
            continue;
        }
        let destination = crate::translate::destination_for(&file.path, to)?;
        if !claimed.insert(destination.clone()) {
            failed.push((
                file.path.clone(),
                format!(
                    "{} is already claimed by another file in this sweep.",
                    destination.display()
                ),
            ));
            continue;
        }
        if crate::vfs::exists(&destination) && !force {
            occupied.push(destination);
            continue;
        }
        match crate::transpile::plan_to_in_context(
            &file.path,
            to,
            Some(&destination),
            force,
            &context,
            &modules,
        ) {
            Ok(plan) => {
                translated.push(file.path.clone());
                fidelity.functions += plan.fidelity.functions;
                fidelity.records += plan.fidelity.records;
                fidelity.constants += plan.fidelity.constants;
                fidelity.newtypes += plan.fidelity.newtypes;
                fidelity.sums += plan.fidelity.sums;
                fidelity.signatures_complete += plan.fidelity.signatures_complete;
                fidelity.signatures_with_foreign_types +=
                    plan.fidelity.signatures_with_foreign_types;
                fidelity.carried_verbatim += plan.fidelity.carried_verbatim;
                edits.extend(plan.edits);
            }
            Err(error) => failed.push((file.path.clone(), error.to_string())),
        }
    }

    if cli.json {
        let outcomes = crate::edit::plan(&edits, crate::edit::Validation::ReparseStrict)?;
        let changes: Vec<_> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "file": o.path,
                    "path": o.path,
                    "diff": workspace_diff(cli, o),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "target": to.name(),
                "translated": translated,
                "functions": fidelity.functions,
                "records": fidelity.records,
                "constants": fidelity.constants,
                "newtypes": fidelity.newtypes,
                "sums": fidelity.sums,
                "signatures_complete": fidelity.signatures_complete,
                "signatures_with_foreign_types": fidelity.signatures_with_foreign_types,
                "carried_verbatim": fidelity.carried_verbatim,
                "already_target": already_target,
                "unreadable": unreadable,
                "destination_exists": occupied,
                "failed": failed
                    .iter()
                    .map(|(p, e)| serde_json::json!({ "file": p, "path": p, "error": e }))
                    .collect::<Vec<_>>(),
                "applied": write,
                "changes": changes,
            }))?
        );
        if write && !outcomes.is_empty() {
            crate::edit::commit(&outcomes)?;
        }
        return Ok(());
    }

    println!(
        "{} file(s) under {} translate to {to}: {} function(s), {} record(s), {} \
         constant(s), {} construct(s) carried.",
        translated.len(),
        dir.display(),
        fidelity.functions,
        fidelity.records,
        fidelity.constants,
        fidelity.carried_verbatim
    );
    if already_target > 0 {
        println!("  {already_target} file(s) are already {to}.");
    }
    for (name, count) in &unreadable {
        println!("  {count} {name} file(s) have no reader, so this skipped them.");
    }
    for destination in &occupied {
        println!(
            "  {} already exists, so this skipped its source. --force overwrites.",
            destination.display()
        );
    }
    for (path, error) in &failed {
        println!("  {} failed: {error}", path.display());
    }
    if translated.is_empty() {
        println!("Nothing to translate.");
        return Ok(());
    }
    let summary = format!(
        "{} file(s) translated to {to} under {}",
        translated.len(),
        dir.display()
    );
    present(cli, None, &edits, &summary, write)
}

/// `fr translate <route.ts> fastapi`, a Next.js API route as a FastAPI module.
fn cmd_translate_fastapi(
    cli: &Cli,
    path: &std::path::Path,
    write: bool,
    out: Option<&std::path::Path>,
    force: bool,
) -> Result<()> {
    let plan = crate::transpile::nextjs::plan_to(path, out, force)?;
    if cli.json {
        let summary = format!(
            "{} translated to FastAPI at {}",
            plan.source.display(),
            plan.destination.display()
        );
        return present_translation(
            cli,
            &plan.edits,
            &plan.fidelity,
            &summary,
            write,
            |report| {
                report["target"] = serde_json::json!("fastapi");
                report["route"] = serde_json::json!(plan.route);
                report["methods"] = serde_json::json!(plan.methods);
            },
        );
    }
    {
        // One entry per URL.
        let served: Vec<String> = plan
            .endpoints
            .iter()
            .map(|(method, route)| format!("{method} {route}"))
            .collect();
        println!(
            "{} -> {} serving {}",
            plan.source.display(),
            plan.destination.display(),
            served.join(", ")
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
    present(cli, None, &plan.edits, &summary, write)
}

/// `fr translate <openapi.yaml> fastapi|nextjs`, a service skeleton from a contract.
fn cmd_scaffold(
    cli: &Cli,
    path: &std::path::Path,
    target: crate::transpile::scaffold::Target,
    write: bool,
    out: Option<&std::path::Path>,
    force: bool,
) -> Result<()> {
    let plan = crate::transpile::scaffold::plan_to(path, target, out, force)?;
    let summary = format!(
        "{} scaffolded into {} file(s)",
        plan.source.display(),
        plan.files.len()
    );
    if cli.json {
        let files: Vec<serde_json::Value> = plan
            .files
            .iter()
            .map(|file| {
                serde_json::json!({
                    "file": file.destination.display().to_string(),
                    "endpoints": file.endpoints,
                })
            })
            .collect();
        let outcomes = crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict)?;
        let changes: Vec<_> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "file": o.path,
                    "diff": workspace_diff(cli, o),
                })
            })
            .collect();
        let report = serde_json::json!({
            "summary": summary,
            "source": plan.source.display().to_string(),
            "target": format!("{target:?}"),
            "files": files,
            "notes": plan.notes,
            "files_changed": outcomes.len(),
            "applied": write,
            "changes": changes,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        if write {
            crate::edit::commit(&outcomes)?;
        }
        return Ok(());
    }
    println!("{} -> {} file(s)", plan.source.display(), plan.files.len());
    for file in &plan.files {
        let served: Vec<String> = file
            .endpoints
            .iter()
            .map(|(method, route)| format!("{method} {route}"))
            .collect();
        println!("  {} ({})", file.destination.display(), served.join(", "));
    }
    for note in &plan.notes {
        println!("  {note}");
    }
    println!();
    present(cli, None, &plan.edits, &summary, write)
}

/// `fr translate <app.py> nextjs`, a FastAPI application as a Next.js route tree.
fn cmd_translate_nextjs(
    cli: &Cli,
    path: &std::path::Path,
    write: bool,
    out: Option<&std::path::Path>,
    force: bool,
) -> Result<()> {
    let plan = crate::transpile::fastapi::plan_to(path, out, force)?;
    let summary = format!(
        "{} translated to {} Next.js route(s)",
        plan.source.display(),
        plan.routes.len()
    );
    if cli.json {
        let routes: Vec<serde_json::Value> = plan
            .routes
            .iter()
            .map(|route| {
                serde_json::json!({
                    "file": route.destination.display().to_string(),
                    "route": route.route,
                    "methods": route.methods,
                })
            })
            .collect();
        return present_translation(
            cli,
            &plan.edits,
            &plan.fidelity,
            &summary,
            write,
            |report| {
                report["target"] = serde_json::json!("nextjs");
                report["routes"] = serde_json::json!(routes);
                report["notes"] = serde_json::json!(plan.notes);
            },
        );
    }
    println!(
        "{} -> {} route file(s)",
        plan.source.display(),
        plan.routes.len()
    );
    for route in &plan.routes {
        println!(
            "  {} serving {} ({})",
            route.destination.display(),
            route.route,
            route.methods.join(", ")
        );
    }
    let f = &plan.fidelity;
    println!(
        "  {} handler(s), {} model(s) across {} file(s)",
        f.functions,
        f.records,
        plan.routes.len()
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
    }
    for note in &plan.notes {
        println!("  {note}");
    }
    println!();
    present(cli, None, &plan.edits, &summary, write)
}

/// `fr openapi`: the contract a Next.js tree declares, before any rewrite.
fn cmd_openapi(cli: &Cli, out: Option<&std::path::Path>, yaml: bool) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let scanned = scan(&root, &scan_options(cli, &[])?)?;
    let files: Vec<PathBuf> = scanned.files.iter().map(|f| f.path.clone()).collect();

    let title = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());
    // Either side of the crossing.
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

    // The same document, spelled the way the reader asked for.
    let text = match yaml {
        true => serde_yaml::to_string(&baseline.document)?,
        false => serde_json::to_string_pretty(&baseline.document)?,
    };
    if let Some(path) = out {
        crate::vfs::write(path, format!("{text}\n"))?;
        if !cli.json {
            println!(
                "{} route file(s) from a {side} -> {}",
                baseline.routes.len(),
                path.display()
            );
        }
    }
    if cli.json {
        // A human run keeps the notes on stderr so the document stays a document.
        let mut payload = baseline.document.clone();
        payload["notes"] = serde_json::json!(baseline.notes);
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if out.is_none() {
        println!("{text}");
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
             defect until argued otherwise, and the one this catches is a contract that \n\
             quietly got smaller."
        );
    }
    Ok(())
}

/// `fr recipe <file>`, run a refactoring written down.
/// The rewrites this build has, for clap to list and to validate against.
fn rewrite_names() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(
        crate::refactor::rewrite::Rewrite::ALL
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>(),
    )
}

fn cmd_recipe(
    cli: &Cli,
    file: Option<&std::path::Path>,
    write: bool,
    explain: bool,
    catalogs: &[PathBuf],
    vocabulary: bool,
) -> Result<()> {
    if vocabulary {
        let words = crate::recipe::vocabulary();
        match cli.json {
            true => println!("{}", serde_json::to_string_pretty(&words)?),
            false => print!("{}", crate::recipe::render(&words)),
        }
        return Ok(());
    }
    let Some(file) = file else {
        return Err(anyhow::anyhow!(
            "name a recipe file, or ask for `--vocabulary`"
        ));
    };
    let root = workspace_root(cli);
    // A relative path is relative to the workspace, as every other file argument is.
    let recipe_path = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.join(file)
    };
    let text = crate::vfs::read_to_string(&recipe_path)
        .with_context(|| format!("reading {}", recipe_path.display()))?;
    let parsed = crate::recipe::parse(&text)?;

    if explain {
        // Accepting `--write` here and applying nothing would be a silent no-op.
        if write {
            return Err(Fault::invalid_input(
                "--explain runs nothing, so there is nothing for --write to apply; \
                 drop one of the two."
                    .to_string(),
            ));
        }
        return explain_recipes(cli, &parsed);
    }

    let scanned = scan(&root, &scan_options(cli, &[])?)?;
    let mut sources = std::collections::BTreeMap::new();
    for source_file in &scanned.files {
        let text = crate::vfs::read_to_string(&source_file.path)?;
        sources.insert(source_file.path.clone(), (source_file.language, text));
    }

    let options = crate::recipe::Options {
        root: &root,
        catalogs,
    };
    let (mut report, after) = crate::recipe::run_file(&parsed, sources.clone(), &options)?;
    crate::vfs::use_filesystem();

    let mut edits = crate::edit::EditSet::new();
    for (path, (language, text)) in &after {
        let before = sources.get(path).map(|(_, t)| t.as_str()).unwrap_or("");
        if before != text {
            edits.add(
                path.clone(),
                crate::edit::Edit::new(
                    crate::span::Span::new(0, before.len()),
                    text,
                    "recipe transaction",
                ),
            );
            edits.declare_language(path.clone(), *language);
        }
    }
    let apply_this = write && report.ok;
    report.applied = apply_this && !edits.is_empty();
    report.rolled_back = write && !report.ok;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        if apply_this {
            crate::edit::commit(&crate::edit::plan(
                &edits,
                crate::edit::Validation::ReparseStrict,
            )?)?;
        }
    } else {
        print_recipe_transaction_outcome(&report);
        if report.rolled_back {
            let outcomes = crate::edit::plan(&edits, crate::edit::Validation::ReparseStrict)?;
            for outcome in &outcomes {
                print!("{}", workspace_diff(cli, outcome));
            }
            println!(
                "\nThe transaction failed, so this wrote nothing. The diff above is what \
                 the complete recipe file would have done."
            );
        } else {
            present(cli, None, &edits, "recipe transaction", apply_this)?;
        }
    }

    if report.stopped_by_refusal {
        std::process::exit(5);
    }
    if !report.ok {
        std::process::exit(1);
    }
    Ok(())
}

struct RecipeFormatFile {
    path: PathBuf,
    formatted: String,
    changed: bool,
}

fn cmd_recipe_fmt(cli: &Cli, inputs: &[PathBuf], write: bool, check: bool) -> Result<()> {
    if write && check {
        return Err(Fault::invalid_input(
            "`fr recipe fmt` cannot both replace a file and only check it; drop --write or --check."
                .to_string(),
        ));
    }
    let files = recipe_format_paths(cli, inputs)?;
    let mut formatted = Vec::with_capacity(files.len());
    for path in files {
        let before = crate::vfs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let after = crate::recipe::format_source(&before)
            .with_context(|| format!("formatting {}", path.display()))?;
        formatted.push(RecipeFormatFile {
            path,
            changed: before != after,
            formatted: after,
        });
    }

    if check {
        let needing_format = formatted
            .iter()
            .filter(|file| file.changed)
            .collect::<Vec<_>>();
        if needing_format.is_empty() {
            match cli.json {
                true => print_recipe_format_json(&formatted)?,
                false if formatted.len() == 1 => println!(
                    "{} is already in canonical recipe layout.",
                    formatted[0].path.display()
                ),
                false => println!(
                    "{} recipe files are already in canonical recipe layout.",
                    formatted.len()
                ),
            }
            return Ok(());
        }
        if needing_format.len() > 1 {
            anyhow::bail!(
                "{} recipe files are not in canonical recipe layout:\n{}\nRun `fr recipe fmt <paths> --write`.",
                needing_format.len(),
                needing_format
                    .iter()
                    .map(|file| format!("  {}", file.path.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        let file = needing_format[0];
        anyhow::bail!(
            "{} is not in canonical recipe layout. Run `fr recipe fmt {}`.",
            file.path.display(),
            file.path.display()
        );
    }
    if write {
        for file in &formatted {
            crate::vfs::write(&file.path, &file.formatted)
                .with_context(|| format!("writing {}", file.path.display()))?;
        }
        match cli.json {
            true => print_recipe_format_json(&formatted)?,
            false => {
                for file in &formatted {
                    println!("formatted {}", file.path.display());
                }
            }
        }
    } else if cli.json {
        print_recipe_format_json(&formatted)?;
    } else {
        for (index, file) in formatted.iter().enumerate() {
            if formatted.len() > 1 {
                if index > 0 {
                    println!();
                }
                println!("==> {} <==", file.path.display());
            }
            print!("{}", file.formatted);
        }
    }
    Ok(())
}

fn recipe_format_paths(cli: &Cli, inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let root = workspace_root(cli);
    let mut files = BTreeSet::new();
    for input in inputs {
        let path = if input.is_absolute() {
            input.to_path_buf()
        } else {
            root.join(input)
        };
        let metadata =
            std::fs::metadata(&path).with_context(|| format!("reading {}", path.display()))?;
        if metadata.is_file() {
            files.insert(path);
            continue;
        }
        let mut found = false;
        let walker = ignore::WalkBuilder::new(&path)
            .standard_filters(!cli.no_ignore)
            .hidden(!cli.no_ignore)
            .git_ignore(!cli.no_ignore)
            .require_git(false)
            .build();
        for entry in walker {
            let entry = entry?;
            if entry.file_type().is_some_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "recipe")
            {
                found = true;
                files.insert(entry.into_path());
            }
        }
        if !found {
            anyhow::bail!("{} contains no .recipe files", path.display());
        }
    }
    Ok(files.into_iter().collect())
}

fn print_recipe_format_json(formatted: &[RecipeFormatFile]) -> Result<()> {
    let files = formatted
        .iter()
        .map(|file| {
            serde_json::json!({
                "path": file.path,
                "formatted": file.formatted,
                "changed": file.changed,
            })
        })
        .collect::<Vec<_>>();
    let changed = formatted.iter().filter(|file| file.changed).count();
    let report = match files.as_slice() {
        [file] => file.clone(),
        _ => serde_json::json!({
            "files": files,
            "files_checked": formatted.len(),
            "files_needing_format": changed,
        }),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// `fr recipe --explain`: the plan as parsed, with nothing selected and nothing run.
fn explain_recipes(cli: &Cli, parsed: &crate::recipe::File) -> Result<()> {
    use crate::recipe::{Expect, OnRefusal, Predicate, Requirement};

    let requirement = |requirement: &Requirement| match requirement {
        Requirement::Language { name, .. } => format!("language {name}"),
        Requirement::Symbol {
            names, selector, ..
        } => {
            let head = match names.as_slice() {
                [name] => format!("symbol \"{name}\""),
                _ => format!(
                    "any symbol {}",
                    names
                        .iter()
                        .map(|name| format!("\"{name}\""))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            };
            let selector = selector
                .iter()
                .map(Predicate::describe)
                .collect::<Vec<_>>()
                .join(" ");
            if selector.is_empty() {
                head
            } else {
                format!("{head} where {selector}")
            }
        }
        Requirement::Path { path, .. } => format!("path \"{path}\""),
    };
    let expectation = |expect: &Expect| match expect {
        Expect::NoNew(what) => format!("no-new {what}"),
        Expect::Matched { how, count } => format!("matched {} {count}", how.as_str()),
        Expect::Applied { how, count } => format!("applied {} {count}", how.as_str()),
        Expect::Changed { how, count } => format!("changed {} {count} files", how.as_str()),
        Expect::Refusals { how, count } => format!("refusals {} {count}", how.as_str()),
        Expect::Step {
            target,
            measure,
            how,
            count,
            ..
        } => format!(
            "step {} {} {} {count}{}",
            target.describe(),
            measure.name(),
            how.as_str(),
            if measure.has_file_unit() {
                " files"
            } else {
                ""
            }
        ),
    };

    if cli.json {
        // Each selector and expectation rides both ways.
        let predicate_json = |predicate: &Predicate| match predicate {
            Predicate::Equals { field, value } => serde_json::json!({
                "field": field, "op": "=", "value": value,
            }),
            Predicate::Glob { field, pattern } => serde_json::json!({
                "field": field, "op": "~", "value": pattern,
            }),
            Predicate::Flag { field, expected } => serde_json::json!({
                "field": field, "op": "flag", "value": expected,
            }),
        };
        let requirement_json = |requirement: &Requirement| match requirement {
            Requirement::Language { name, .. } => serde_json::json!({
                "kind": "language", "name": name,
            }),
            Requirement::Symbol {
                names, selector, ..
            } => serde_json::json!({
                "kind": if names.len() == 1 { "symbol" } else { "any-symbol" },
                "names": names,
                "selector_parts": selector.iter().map(predicate_json).collect::<Vec<_>>(),
            }),
            Requirement::Path { path, .. } => serde_json::json!({
                "kind": "path", "path": path,
            }),
        };
        let expect_json = |expect: &Expect| match expect {
            Expect::NoNew(what) => serde_json::json!({
                "predicate": "no-new", "op": "=", "value": what,
            }),
            Expect::Matched { how, count } => serde_json::json!({
                "predicate": "matched", "op": how.as_str(), "value": count,
            }),
            Expect::Applied { how, count } => serde_json::json!({
                "predicate": "applied", "op": how.as_str(), "value": count,
            }),
            Expect::Changed { how, count } => serde_json::json!({
                "predicate": "changed", "op": how.as_str(), "value": count, "unit": "files",
            }),
            Expect::Refusals { how, count } => serde_json::json!({
                "predicate": "refusals", "op": how.as_str(), "value": count,
            }),
            Expect::Step {
                target,
                measure,
                how,
                count,
                ..
            } => {
                let mut value = serde_json::json!({
                    "predicate": measure.name(),
                    "op": how.as_str(),
                    "value": count,
                });
                match target {
                    crate::recipe::StepTarget::Number(step) => {
                        value["step"] = serde_json::json!(step)
                    }
                    crate::recipe::StepTarget::Id(id) => value["step_id"] = serde_json::json!(id),
                }
                if measure.has_file_unit() {
                    value["unit"] = serde_json::json!("files");
                }
                value
            }
        };
        let recipes: Vec<_> = parsed
            .recipes
            .iter()
            .map(|recipe| {
                serde_json::json!({
                    "recipe": recipe.name,
                    "description": recipe.description,
                    "requires": recipe.requires.iter().map(requirement).collect::<Vec<_>>(),
                    "requirement_parts": recipe.requires.iter().map(requirement_json)
                        .collect::<Vec<_>>(),
                    "steps": recipe.steps.iter().map(|step| serde_json::json!({
                        "step": step.operation.describe(),
                        "selector": selector_of(step),
                        "selector_parts": step.selector.iter().map(predicate_json)
                            .collect::<Vec<_>>(),
                        "on_refusal": match step.on_refusal {
                            OnRefusal::Stop => "stop",
                            OnRefusal::Report => "report",
                            OnRefusal::Allow => "allow",
                        },
                        "limit": step.limit,
                        "allow_empty": step.allow_empty,
                        "id": step.id,
                    })).collect::<Vec<_>>(),
                    "expectations": recipe.expects.iter().map(expectation).collect::<Vec<_>>(),
                    "expectations_parts": recipe.expects.iter().map(expect_json)
                        .collect::<Vec<_>>(),
                    "ran": false,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&recipes)?);
        return Ok(());
    }

    for recipe in &parsed.recipes {
        println!("recipe {}: {} step(s)", recipe.name, recipe.steps.len());
        if let Some(description) = &recipe.description {
            println!("  {description}");
        }
        for requires in &recipe.requires {
            println!("  requires {}", requirement(requires));
        }
        println!();
        for (i, step) in recipe.steps.iter().enumerate() {
            let selector = selector_of(step);
            println!(
                "  {}{}  {}{}",
                i + 1,
                step.id
                    .as_ref()
                    .map(|id| format!(" [{id}]"))
                    .unwrap_or_default(),
                step.operation.describe(),
                if selector.is_empty() {
                    String::new()
                } else {
                    format!(" where {selector}")
                }
            );
            let mut notes: Vec<String> = Vec::new();
            match step.on_refusal {
                OnRefusal::Stop => {}
                OnRefusal::Report => notes.push("on-refusal report".to_string()),
                OnRefusal::Allow => notes.push("on-refusal allow".to_string()),
            }
            if let Some(limit) = step.limit {
                notes.push(format!("limit {limit}"));
            }
            if step.allow_empty {
                notes.push("allow-empty".to_string());
            }
            if !notes.is_empty() {
                println!("     {}", notes.join(", "));
            }
        }
        if !recipe.expects.is_empty() {
            println!("\nexpect");
            for expect in &recipe.expects {
                println!("  {}", expectation(expect));
            }
        }
        println!();
    }
    println!("Parsed and not run. Run without --explain to see matches and a diff.");
    Ok(())
}

/// A step's `where` clause as one line, the same spelling the run report prints.
fn selector_of(step: &crate::recipe::Step) -> String {
    step.selector
        .iter()
        .map(|p| p.describe())
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_recipe_outcome(report: &crate::recipe::Report) {
    // The header describes the file, so it counts the steps the recipe holds.
    println!(
        "recipe {}: {} step(s)",
        report.recipe, report.steps_in_recipe
    );
    if let Some(description) = &report.description {
        println!("  {description}");
    }
    if report.steps.len() < report.steps_in_recipe {
        println!("  the run reached {} of them", report.steps.len());
    }
    println!();
    for (i, step) in report.steps.iter().enumerate() {
        println!(
            "  {}{}  {}{}",
            i + 1,
            step.id
                .as_ref()
                .map(|id| format!(" [{id}]"))
                .unwrap_or_default(),
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
        for created in &step.files_created {
            println!("       created  {}", created.display());
        }
        for refusal in &step.refusals {
            println!("       refused  {}: {}", refusal.subject, refusal.reason);
        }
        for warning in &step.warnings {
            println!("       left     {}", warning.describe());
        }
    }
    if let Some(why) = &report.stopped {
        println!("\nstopped: {why}");
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

fn print_recipe_transaction_outcome(report: &crate::recipe::WorkspaceReport) {
    println!(
        "recipe transaction: schema {}, {} recipe(s), {} file(s) changed.",
        report.schema,
        report.recipes.len(),
        report.files_changed
    );
    for recipe in &report.recipes {
        println!();
        print_recipe_outcome(recipe);
    }
    if let Some(recipe) = &report.failed_recipe {
        println!("transaction failed in recipe {recipe}");
    } else {
        println!("transaction ready: every recipe completed");
    }
    if !report.files_created.is_empty() {
        println!("files created:");
        for path in &report.files_created {
            println!("  {}", path.display());
        }
    }
    println!();
}

fn cmd_remove_flag(cli: &Cli, flag: &str, value: FlagValue, write: bool) -> Result<()> {
    use crate::refactor::cascade::FlagTarget;

    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    // A position, for the ambiguity the refusal tells the reader to resolve this way.
    let target = match parse_target_position(cli, flag)? {
        Some(pos) => {
            let path = workspace_path(cli, &pos.path)?;
            let source = crate::vfs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let offset = offset_at(&source, pos.line, pos.col, &path)?;
            FlagTarget::At(path, offset)
        }
        None => FlagTarget::Named(flag.to_string()),
    };

    // The cascade reports a name nothing declares as an untyped error, and the exit code
    // follows the error's type.
    let index = build_index(cli, &[])?;
    if let FlagTarget::Named(name) = &target {
        if index.symbols_written(name, None).is_empty() {
            let near = nearest_names(&index, name);
            return Err(Fault::not_found_near(
                format!(
                    "no symbol named '{name}' to remove; this changed nothing.{}",
                    did_you_mean(&near)
                ),
                near,
            ));
        }
    }
    let plan = crate::refactor::cascade::remove_flag_for(&root, &target, value.0)?;

    if plan.is_empty() {
        println!("Removing {flag} as {} changes nothing.", value.0);
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
    present(cli, Some(&index), &plan.edits, &summary, write)
}

fn cmd_rewrite(cli: &Cli, target: &str, name: Option<&str>, write: bool) -> Result<()> {
    use crate::refactor::rewrite::{self, Rewrite};

    let pos = parse_position(target).ok_or_else(|| {
        Fault::invalid_input("a rewrite needs a position, as path:line:col.".to_string())
    })?;
    refuse_zero_column(&pos)?;
    let path = workspace_path(cli, &pos.path)?;
    let source =
        crate::vfs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let offset = offset_at(&source, pos.line, pos.col, &path)?;

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
    present(cli, Some(&index), &plan.edits, &summary, write)
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

    // A site the template cannot cover still matches, so name it before anything else.
    let root = workspace_root(cli);
    if !cli.json {
        print!(
            "{}",
            describe_skipped_occurrences(&root, &plan.skipped_with_comments)
        );
    }

    // A pattern that matched nowhere found nothing, which is the failure `fr rename` reports
    // with the same code.
    if plan.matches.is_empty() && plan.skipped_with_comments.is_empty() {
        return Err(Fault::not_found(format!(
            "no {lang} code matches `{pattern}`; this changed nothing."
        )));
    }

    let summary = format!(
        "rewrote {} occurrence(s) of `{}` in {} file(s)",
        plan.matches.len(),
        plan.pattern,
        plan.edits.file_count()
    );
    present_with(cli, Some(&index), &plan.edits, &summary, write, |report| {
        report["skipped_occurrences"] = skipped_occurrences_json(&plan.skipped_with_comments);
    })
}

/// The matches a template could not cover, as data.
fn skipped_occurrences_json(skipped: &[(std::path::PathBuf, usize)]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = skipped
        .iter()
        .map(|(path, offset)| {
            let at = crate::vfs::read_to_string(path)
                .ok()
                .map(|source| LineIndex::new(&source).line_col(*offset, &source));
            serde_json::json!({
                "file": path,
                "line": at.as_ref().map(|a| a.line),
                "col": at.as_ref().map(|a| a.col),
                "reason": "the match holds a comment, which the template has nowhere to put",
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

/// The report for matches the rewrite could not cover.
fn describe_skipped_occurrences(
    root: &std::path::Path,
    skipped: &[(std::path::PathBuf, usize)],
) -> String {
    if skipped.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "skipped {} occurrence(s) containing comments, which a template has nowhere to \
         put and a rewrite would delete",
        skipped.len()
    );
    out.push('\n');
    for (path, offset) in skipped {
        let shown = shown_path(root, path);
        match crate::vfs::read_to_string(path) {
            Ok(source) => {
                let at = LineIndex::new(&source).line_col(*offset, &source);
                out.push_str(&format!("  {shown}:{}:{}\n", at.line, at.col));
            }
            Err(_) => out.push_str(&format!("  {shown}\n")),
        }
    }
    out
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

    // `-f`/`--set` describe a helm invocation.
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
            print!("{}", result.format_tree_under(&workspace_root(cli)));
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
        let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
        let steps: Vec<_> = result
            .steps
            .iter()
            .map(|s| {
                let source = sources
                    .entry(s.file.clone())
                    .or_insert_with(|| crate::vfs::read_to_string(&s.file).unwrap_or_default());
                let at = LineIndex::new(source).line_col(s.span.start, source);
                serde_json::json!({
                    "text": s.text,
                    "file": s.file,
                    "line": at.line,
                    "col": at.col,
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
        // The provenance answer names its model, and a reader dispatching on the
        // shape needs this side to name its own as well.
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": symbol.qualified_name(),
                "direction": direction,
                "model": "value-flow",
                "steps": steps,
                "stops": stops,
            }))?
        );
    } else {
        print!("{}", result.format_tree_under(&workspace_root(cli)));
    }
    Ok(())
}

/// Say which supplied input decided each Helm competition, and what the answer still rests on.
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
                "  {} decided by {}: {}",
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

fn cmd_stitch(
    cli: &Cli,
    env: Option<&str>,
    orphaned_only: bool,
    files: bool,
    flags: bool,
) -> Result<()> {
    use crate::analysis::stitch;

    let index = build_index(cli, &[])?;
    // The other half of what configuration names: a file, rather than a value a program reads.
    if files {
        return report_path_links(cli, &index);
    }
    // And the third thing configuration names: a flag on a command line, which
    // some program declares.
    if flags {
        return report_flags(cli, &index);
    }
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
        "{} chain(s). The link from a manifest to a program is the variable's name. \n\
         It is a string on both sides, and nothing can prove the two refer to one \n\
         variable, so this reports those hops as name-only.",
        chains.len()
    );
    Ok(())
}

/// Every path a configuration file writes, and the file it names.
fn report_path_links(cli: &Cli, index: &crate::index::Index) -> Result<()> {
    let root = workspace_root(cli);
    let links = crate::analysis::paths::links(index, &root)?;

    if cli.json {
        let payload: Vec<_> = links
            .iter()
            .map(|l| {
                serde_json::json!({
                    "from": l.from,
                    "line": l.line,
                    "language": l.language.name(),
                    "written": l.written,
                    "names": l.names,
                    "dangling": l.is_dangling(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if links.is_empty() {
        println!("No configuration file names a path in this workspace.");
        return Ok(());
    }
    for link in &links {
        let names = match &link.names {
            Some(file) => file.display().to_string(),
            None => "nothing in this workspace".to_string(),
        };
        println!(
            "{}:{} runs {} -> {names}",
            link.from.display(),
            link.line,
            link.written
        );
    }
    let dangling = links.iter().filter(|l| l.is_dangling()).count();
    println!(
        "{} path(s), {dangling} naming nothing. A path either exists or it \n\
         does not. So these are exact, and not name-only.",
        links.len()
    );
    Ok(())
}

/// Every flag this workspace declares or passes, and where each is.
fn report_flags(cli: &Cli, index: &crate::index::Index) -> Result<()> {
    let flags = crate::analysis::flags::flags(index)?;

    if cli.json {
        let payload: Vec<_> = flags
            .iter()
            .map(|f| {
                serde_json::json!({
                    "flag": f.flag,
                    "declared": f.declared.iter().map(|d| serde_json::json!({
                        "file": d.file,
                        "line": d.line,
                        "language": d.language.name(),
                    })).collect::<Vec<_>>(),
                    "passed": f.passed.iter().map(|p| serde_json::json!({
                        "file": p.file,
                        "line": p.line,
                        "language": p.language.name(),
                    })).collect::<Vec<_>>(),
                    "undeclared": f.is_undeclared(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if flags.is_empty() {
        println!("No command-line flag turns up in this workspace, declared or passed.");
        return Ok(());
    }
    for flag in &flags {
        let where_ = match flag.declared.first() {
            Some(d) => format!("{}:{}", d.file.display(), d.line),
            None => "nothing declares it".to_string(),
        };
        println!(
            "--{} declared {where_}, passed {} time(s)",
            flag.flag,
            flag.passed.len()
        );
    }
    let undeclared = flags.iter().filter(|f| f.is_undeclared()).count();
    println!(
        "{} flag(s), {undeclared} passed and declared nowhere. The link is the \n\
         flag's name, a string on both sides, so every hop is name-only.",
        flags.len()
    );
    Ok(())
}

fn cmd_impact(cli: &Cli, target: &str, caller_depth: usize) -> Result<()> {
    use crate::analysis::impact;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let graph = build_call_graph(cli, &index);
    let result = impact::analyse_with_graph(&index, symbol.id, caller_depth, &graph)?;

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
    if dot && cli.json {
        anyhow::bail!("graph prints one format at a time; drop --dot or --json.");
    }
    let index = build_index(cli, &[])?;
    let graph = build_call_graph(cli, &index);
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());

    if dot {
        print!("{}", graph.to_dot(&index, &root));
        return Ok(());
    }

    let breakdown = graph.confidence_breakdown();
    let by_origin = graph.origin_breakdown();
    if cli.json {
        // The counts say how big the graph is and nothing about its shape, and anything reading
        // this wants the edges.
        let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
        let nodes: Vec<_> = graph
            .nodes()
            .into_iter()
            .filter_map(|id| index.symbol(id))
            .map(|s| {
                let source = sources
                    .entry(s.file.clone())
                    .or_insert_with(|| crate::vfs::read_to_string(&s.file).unwrap_or_default());
                let at = LineIndex::new(source).line_col(s.name_span.start, source);
                serde_json::json!({
                    "id": s.id,
                    "name": s.qualified_name(),
                    "file": s.file.strip_prefix(&root).unwrap_or(&s.file),
                    "line": at.line,
                    "kind": s.kind,
                })
            })
            .collect();
        let edges: Vec<_> = graph
            .edges()
            .into_iter()
            .map(|(from, to, edge)| {
                serde_json::json!({
                    "from": from,
                    "to": to,
                    "confidence": edge.confidence.as_str(),
                    "origin": edge.origin.as_str(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "functions": graph.node_count(),
                "calls": graph.edge_count(),
                "hierarchy_edges": graph.hierarchy_edge_count(),
                "unresolved_calls": graph.unresolved.len(),
                "file_scope_calls": graph.file_scope.len(),
                "by_confidence": breakdown,
                "by_origin": by_origin,
                "nodes": nodes,
                "edges": edges,
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
    println!("file-scope calls  {}", graph.file_scope.len());

    // The report names every call site the dispatch scan and the index disagree about.
    if !graph.hierarchy_gaps.is_empty() {
        println!(
            "\n{} call site(s) the hierarchy scan could not line up with the index:",
            graph.hierarchy_gaps.len()
        );
        for (file, detail) in graph.hierarchy_gaps.iter().take(10) {
            println!("  {}: {detail}", shown_path(&root, file));
        }
        if graph.hierarchy_gaps.len() > 10 {
            println!("  … and {} more", graph.hierarchy_gaps.len() - 10);
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
        let graph = build_call_graph(cli, &index);
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
            let root = workspace_root(cli);
            println!(
                "{} function(s) not reachable from any of {} entry point(s):",
                orphans.len(),
                entries.len()
            );
            for s in &orphans {
                println!(
                    "  {:<40} {}",
                    s.qualified_name(),
                    shown_path(&root, &s.file)
                );
            }
            println!(
                "\nNote: reachability follows resolved call edges plus class-hierarchy \
                 dispatch, so it counts a method that a trait object or an interface \
                 value reaches. It misses a function held in a map or a struct field, \
                 and a name assembled at runtime. This list can still name a function \
                 that something calls."
            );
        }
        return Ok(());
    }

    if cli.json {
        // Same reason as `fr unused`: a name alone does not name a symbol, and anything
        // reading this wants somewhere to go.
        let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
        let payload: Vec<_> = selected
            .iter()
            .filter_map(|e| {
                index.symbol(e.symbol).map(|s| {
                    let source = sources
                        .entry(s.file.clone())
                        .or_insert_with(|| crate::vfs::read_to_string(&s.file).unwrap_or_default());
                    let at = LineIndex::new(source).line_col(s.name_span.start, source);
                    serde_json::json!({
                        "name": s.qualified_name(),
                        "file": s.file,
                        "line": at.line,
                        "col": at.col,
                        "language": s.language.name(),
                        "kind": e.kind.as_str(),
                        "rule": e.rule,
                    })
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        let root = workspace_root(cli);
        for entry in &selected {
            if let Some(symbol) = index.symbol(entry.symbol) {
                println!(
                    "{:<18} {:<32} {}",
                    entry.kind.as_str(),
                    symbol.qualified_name(),
                    shown_path(&root, &symbol.file)
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

    refuse_stale_plan(&index, &plan.edits)?;
    let outcomes = crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict)?;

    if cli.json {
        let files: Vec<_> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "file": o.path,
                    "path": o.path,
                    "diff": workspace_diff(cli, o),
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
                // Definition sites edited, kept beside `reference_edits` so the counts add up
                // for a reader who also ran `fr usages`.
                "definition_edits": plan.edits.edit_count() - plan.reference_edits,
                "applied": write,
                "changes": files,
                "warnings": plan.warnings,
                "skipped_files": skipped_files_json(&index),
                "unparsed_files": unparsed_files_json(&index),
            }))?
        );
        if write {
            crate::edit::commit(&outcomes)?;
        }
        return Ok(());
    }

    for outcome in &outcomes {
        print!("{}", workspace_diff(cli, outcome));
    }

    println!(
        "\n{} → {}: {} site(s) across {} file(s)",
        plan.old_name,
        plan.new_name,
        plan.edits.edit_count(),
        outcomes.len()
    );

    if !plan.warnings.is_empty() {
        let grouped = crate::refactor::rename::group_warnings(&plan.warnings);
        let root = workspace_root(cli);
        // The rename changed each dispatch site, along with the family it can reach, and left
        // the other kinds alone.
        let show = |kind: &str, warnings: &[&crate::refactor::Warning]| {
            println!("  {} ({}):", kind, warnings.len());
            for w in warnings.iter().take(10) {
                println!(
                    "    {}:{}:{}  {}",
                    shown_path(&root, &w.file),
                    w.line,
                    w.col,
                    w.detail
                );
            }
            if warnings.len() > 10 {
                println!("    … and {} more", warnings.len() - 10);
            }
        };
        let (changed, unchanged): (Vec<_>, Vec<_>) = grouped
            .into_iter()
            .partition(|(kind, _)| *kind == "dispatch-candidate");
        if !changed.is_empty() {
            println!("\nChanged with the family. Review these yourself:");
            for (kind, warnings) in &changed {
                show(kind, warnings);
            }
        }
        if !unchanged.is_empty() {
            println!("\nNot changed. Review these yourself:");
            for (kind, warnings) in &unchanged {
                show(kind, warnings);
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

/// What went wrong, named so a program can branch on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultKind {
    NotFound,
    Ambiguous,
    InvalidInput,
}

impl FaultKind {
    fn as_str(self) -> &'static str {
        match self {
            FaultKind::NotFound => "not-found",
            FaultKind::Ambiguous => "ambiguous",
            FaultKind::InvalidInput => "invalid-input",
        }
    }
}

/// One definition an ambiguous name could have meant, carried as data.
#[derive(Debug, Clone)]
struct Candidate {
    name: String,
    kind: &'static str,
    file: PathBuf,
    line: usize,
    col: usize,
}

/// A failure whose kind the JSON error object can name.
#[derive(Debug)]
struct Fault {
    kind: FaultKind,
    message: String,
    candidates: Vec<Candidate>,
    /// Names close to the one nothing matched, nearest first.
    suggestions: Vec<String>,
}

impl Fault {
    fn not_found(message: String) -> anyhow::Error {
        Self::not_found_near(message, Vec::new())
    }

    /// A not-found whose message already asks "did you mean", with the same names as data.
    fn not_found_near(message: String, suggestions: Vec<String>) -> anyhow::Error {
        anyhow::Error::new(Fault {
            kind: FaultKind::NotFound,
            message,
            candidates: Vec::new(),
            suggestions,
        })
    }

    fn invalid_input(message: String) -> anyhow::Error {
        anyhow::Error::new(Fault {
            kind: FaultKind::InvalidInput,
            message,
            candidates: Vec::new(),
            suggestions: Vec::new(),
        })
    }

    fn ambiguous(message: String, candidates: Vec<Candidate>) -> anyhow::Error {
        anyhow::Error::new(Fault {
            kind: FaultKind::Ambiguous,
            message,
            candidates,
            suggestions: Vec::new(),
        })
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Fault {}

/// The failure as one JSON object on stdout, beside the prose on stderr.
fn report_json_error(error: &anyhow::Error) {
    let fault = error.downcast_ref::<Fault>();
    let kind = match fault {
        Some(fault) => fault.kind.as_str(),
        None if error.chain().any(|c| c.is::<crate::refactor::Refusal>()) => "refused",
        None if error.chain().any(|c| c.is::<std::io::Error>()) => "io",
        None => "error",
    };
    let mut object = serde_json::json!({
        "kind": kind,
        "message": format!("{error:#}"),
    });
    if let Some(fault) = fault {
        if !fault.candidates.is_empty() {
            object["candidates"] = fault
                .candidates
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "kind": c.kind,
                        "file": c.file,
                        // The same path under the key older scripts read.
                        "path": c.file,
                        "line": c.line,
                        "col": c.col,
                    })
                })
                .collect();
        }
        if !fault.suggestions.is_empty() {
            object["suggestions"] = serde_json::json!(fault.suggestions);
        }
    }
    // A refusal's blocking positions, as data beside the prose, the way an
    // ambiguity's candidates already ride.
    if let Some(refusal) = crate::refactor::refusal_in(error) {
        let references = refusal.references();
        if !references.is_empty() {
            object["references"] = serde_json::json!(references);
        }
    }
    let payload = serde_json::json!({ "error": object });
    match serde_json::to_string_pretty(&payload) {
        Ok(text) => println!("{text}"),
        Err(error) => {
            println!("{{\"error\":{{\"kind\":\"error\",\"message\":\"unprintable: {error}\"}}}}")
        }
    }
}

/// A position in a file, given as `path:line:col`.
struct Position {
    path: PathBuf,
    line: usize,
    col: usize,
}

/// Refuse a position whose column is 0.
fn refuse_zero_column(pos: &Position) -> Result<()> {
    if pos.col == 0 {
        return Err(Fault::invalid_input(format!(
            "{}:{}:0 names column 0; columns start at 1.",
            pos.path.display(),
            pos.line
        )));
    }
    Ok(())
}

/// Parse `path:line:col`.
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

/// A target meant as a position, parsed, with a malformed one refused.
fn parse_target_position(cli: &Cli, target: &str) -> Result<Option<Position>> {
    if let Some(pos) = parse_position(target) {
        refuse_zero_column(&pos)?;
        return Ok(Some(pos));
    }
    match position_shape_problem(cli, target) {
        Some(problem) => Err(Fault::invalid_input(format!(
            "that looks like a position; {problem}"
        ))),
        None => Ok(None),
    }
}

/// Why a position-shaped target did not parse as one, or `None` for a plain name.
fn position_shape_problem(cli: &Cli, target: &str) -> Option<String> {
    let root = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());
    let is_file = |path: &str| {
        let path = std::path::Path::new(path);
        match path.is_absolute() {
            true => path.is_file(),
            false => root.join(path).is_file(),
        }
    };
    let parts: Vec<&str> = target.rsplitn(3, ':').collect();
    match parts.as_slice() {
        [col, line, path] if is_file(path) => {
            if line.parse::<usize>().is_err() {
                return Some(format!(
                    "'{line}' is not a line number. Positions are path:line:col."
                ));
            }
            if col.parse::<usize>().is_err() {
                return Some(format!(
                    "'{col}' is not a column number. Positions are path:line:col."
                ));
            }
            None
        }
        // Anything after `existing-file:` was meant as a position.
        [tail, path] if is_file(path) && !tail.is_empty() => {
            if tail.chars().all(|c| c.is_ascii_digit()) {
                Some(format!(
                    "'{target}' names a file and a line but no column. Positions are \
                     path:line:col."
                ))
            } else {
                Some(format!(
                    "'{tail}' is not a line number. Positions are path:line:col."
                ))
            }
        }
        _ => None,
    }
}

/// Names in the index within edit distance 2 of `wanted`, nearest first, at most three.
fn nearest_names(index: &Index, wanted: &str) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = index
        .symbols
        .iter()
        .map(|s| s.name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|name| {
            let d = crate::recipe::distance(name, wanted);
            (d <= 2).then_some((d, name))
        })
        .collect();
    scored.sort();
    scored
        .into_iter()
        .take(3)
        .map(|(_, name)| name.to_string())
        .collect()
}

/// "Did you mean 'a', 'b' or 'c'?", or nothing when there is nothing close.
fn did_you_mean(suggestions: &[String]) -> String {
    let quoted: Vec<String> = suggestions.iter().map(|s| format!("'{s}'")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => format!(" Did you mean {one}?"),
        [head @ .., last] => format!(" Did you mean {} or {last}?", head.join(", ")),
    }
}

/// The sites that write this name and reach no definition.
fn naming_nothing(cli: &Cli, index: &Index, target: &str) -> String {
    let root = workspace_root(cli);
    let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    let sites: Vec<String> = index
        .references
        .iter()
        .filter(|r| r.name == target && r.target.is_none())
        .map(|r| {
            let source = sources
                .entry(r.file.clone())
                .or_insert_with(|| crate::vfs::read_to_string(&r.file).unwrap_or_default());
            let at = LineIndex::new(source).line_col(r.span.start, source);
            format!("\n  {}:{at}", shown_path(&root, &r.file))
        })
        .collect();
    if sites.is_empty() {
        return String::new();
    }
    format!(
        "\n{} site(s) name it and reach no definition.{}",
        sites.len(),
        sites.join("")
    )
}

/// Where each rival definition sits, as data for the JSON error object.
fn candidates_of(symbols: &[&Symbol]) -> Vec<Candidate> {
    let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    symbols
        .iter()
        .map(|s| {
            let source = sources
                .entry(s.file.clone())
                .or_insert_with(|| crate::vfs::read_to_string(&s.file).unwrap_or_default());
            let at = LineIndex::new(source).line_col(s.name_span.start, source);
            Candidate {
                name: s.qualified_name(),
                kind: s.kind.as_str(),
                file: s.file.clone(),
                line: at.line,
                col: at.col,
            }
        })
        .collect()
}

/// The likeliest reason a real file is absent from the index.
fn why_unindexed(cli: &Cli, path: &std::path::Path) -> String {
    if crate::lang::detect(path).is_none() {
        return "It is in no language this reads.".to_string();
    }
    let limit = cli
        .max_file_size
        .unwrap_or(crate::scan::ScanOptions::default().max_file_bytes);
    if std::fs::metadata(path).is_ok_and(|m| m.len() > limit) {
        return format!("It is larger than the {limit}-byte limit; raise --max-file-size.");
    }
    "An ignore rule probably excludes it, or --languages narrowed the scan; \
     --no-ignore reads ignored files."
        .to_string()
}

/// Resolve a CLI target to a symbol, accepting either a position or a name.
fn resolve_target<'a>(cli: &Cli, index: &'a Index, target: &str) -> Result<&'a Symbol> {
    if let Some(pos) = parse_target_position(cli, target)? {
        let path = workspace_path(cli, &pos.path)?;
        let source = crate::vfs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let offset = offset_at(&source, pos.line, pos.col, &path)?;

        return index.definition_at(&path, offset).ok_or_else(|| {
            // A file the scan never reached has no symbols at any position.
            Fault::not_found(match index.file(&path) {
                Some(_) => format!(
                    "no symbol or resolved reference at {}:{}:{}",
                    path.display(),
                    pos.line,
                    pos.col
                ),
                None => format!(
                    "{} is not in the workspace this indexed, so nothing in it \
                     resolves. {} `fr scan` lists every file it read.",
                    path.display(),
                    why_unindexed(cli, &path)
                ),
            })
        });
    }

    // A qualified name, `Box::size`, the spelling every listing prints, before a bare one.
    let matches = index.symbols_written(target, None);
    match matches.len() {
        0 => {
            let near = nearest_names(index, target);
            Err(Fault::not_found_near(
                format!(
                    "no symbol named '{target}'.{}{}",
                    did_you_mean(&near),
                    naming_nothing(cli, index, target)
                ),
                near,
            ))
        }
        1 => Ok(matches[0]),
        // Several sites can declare one entity.
        _ if index.is_one_entity(&matches) => Ok(matches[0]),
        // Every candidate answers to the name as written, so listing that name again helps
        // nobody.
        _ if target.contains("::") && matches.iter().all(|s| s.qualified_name() == target) => {
            let mut listing = String::new();
            for symbol in &matches {
                listing.push_str(&format!("\n  {} in {}", target, symbol.file.display()));
            }
            Err(Fault::ambiguous(
                format!(
                    "{} files declare '{target}'; specify a position as \
                     path:line:col{listing}",
                    matches.len()
                ),
                candidates_of(&matches),
            ))
        }
        _ => {
            // This reports the ambiguity and never resolves it by guessing.
            let mut listing = String::new();
            // The listing names each candidate by the name that would select it, so the
            // reader copies a line instead of hunting for a line number.
            for symbol in &matches {
                listing.push_str(&format!(
                    "\n  {} ({}) in {}",
                    symbol.qualified_name(),
                    symbol.kind.as_str(),
                    symbol.file.display()
                ));
            }
            Err(Fault::ambiguous(
                format!(
                    "'{target}' is defined {} times; name one of these, or give a position \
                     as path:line:col{listing}",
                    matches.len()
                ),
                candidates_of(&matches),
            ))
        }
    }
}

fn build_index(cli: &Cli, languages: &[String]) -> Result<Index> {
    use std::io::IsTerminal;

    let options = scan_options(cli, languages)?;
    // Canonicalise the root so indexed paths match the ones commands resolve from
    // arguments; otherwise /var and /private/var name the same file but never match.
    let root = workspace_root(cli);
    let scanned = crate::scan::scan(&root, &options)?;

    let cache = if cli.no_cache {
        None
    } else {
        crate::cache::Cache::open()
    };

    // A cold index of a large workspace takes most of a minute, and the silence read as a hang.
    let tty = std::io::stderr().is_terminal();
    let paint = |done: usize, total: usize| {
        // Repainting on every file would spend more time on the terminal than on a
        // small file, so the counter moves in coarse steps.
        if done.is_multiple_of(16) || done == total {
            eprint!("\rindexing {done}/{total} files…");
        }
    };
    let started = std::time::Instant::now();
    let last_emitted = std::sync::atomic::AtomicU64::new(0);
    let machine_paint = |done: usize, total: usize| {
        use std::sync::atomic::Ordering;
        // Nothing before the second boundary: a small workspace indexes in milliseconds and
        // needs no progress at all.
        let elapsed = started.elapsed().as_secs();
        let previous = last_emitted.load(Ordering::Relaxed);
        if elapsed > previous
            && last_emitted
                .compare_exchange(previous, elapsed, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            eprintln!("{}", indexing_progress_line(done, total));
        }
    };
    let progress: Option<&(dyn Fn(usize, usize) + Sync)> = if tty {
        Some(&paint)
    } else if cli.json {
        Some(&machine_paint)
    } else {
        None
    };
    let index = Index::build_with_cache_reporting(&scanned, cache.as_ref(), progress)?;
    if tty {
        let widest = format!("indexing {0}/{0} files…", scanned.files.len());
        eprint!("\r{:width$}\r", "", width = widest.chars().count());
    }

    if let Some(cache) = &cache {
        let stats = cache.stats();
        let (hits, misses) = (stats.hits, stats.misses);
        tracing::debug!("cache: {hits} hit(s), {misses} miss(es)");
    }
    warn_partial_index(cli, &index);
    Ok(index)
}

fn build_call_graph(cli: &Cli, index: &Index) -> CallGraph {
    if !cli.no_cache {
        if let Some(cache) = crate::cache::Cache::open() {
            return CallGraph::build_cached(index, &cache);
        }
    }
    CallGraph::build(index)
}

/// One progress line for a JSON caller's stderr, while a cold index builds.
fn indexing_progress_line(done: usize, total: usize) -> String {
    serde_json::json!({ "indexing": { "done": done, "total": total } }).to_string()
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
        let known: Vec<_> = Capability::ALL.iter().map(|c| c.label()).collect();
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
            "\nAn entry takes its key from every workspace file, the query set and the analysis \n\
             code, so an edit makes every stale answer unreachable rather than serving a \n\
             result that described an earlier tree."
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
    paths: &[PathBuf],
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
                    "files_by_gap": s.files_by_gap,
                }))?
            );
        } else {
            println!("files       {}", s.files);
            println!("symbols     {}", s.symbols);
            println!("references  {} ({} resolved)", s.references, s.resolved);
            for (confidence, count) in &s.by_confidence {
                println!("  {confidence:<18} {count}");
            }
            for (gap, count) in &s.files_by_gap {
                println!("\n{count} file(s): {gap}; their facts are incomplete");
            }
        }
        return Ok(());
    }

    let roots = absolute_paths(cli, paths)?;
    let selected: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| name_filter.is_none_or(|n| s.name.contains(n)))
        .filter(|s| kind_filter.is_none_or(|k| s.kind.as_str() == k))
        .filter(|s| roots.is_empty() || roots.iter().any(|r| s.file.starts_with(r)))
        .collect();

    if cli.json {
        // Every position this tool *accepts* is a 1-based line and column, so the report says
        // each symbol's extent that way too.
        let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
        let payload: Vec<_> = selected
            .iter()
            .map(|s| {
                let source = sources
                    .entry(s.file.clone())
                    .or_insert_with(|| crate::vfs::read_to_string(&s.file).unwrap_or_default());
                let lines = LineIndex::new(source);
                let at = lines.line_col(s.name_span.start, source);
                let start = lines.line_col(s.full_span.start, source);
                let end = lines.line_col(s.full_span.end, source);
                let name_span =
                    serde_json::json!({ "start": s.name_span.start, "end": s.name_span.end });
                let full_span =
                    serde_json::json!({ "start": s.full_span.start, "end": s.full_span.end });
                serde_json::json!({
                    "name": s.name,
                    "qualified_name": s.qualified_name(),
                    "kind": s.kind.as_str(),
                    "file": s.file,
                    "line": at.line,
                    "col": at.col,
                    "language": s.language.name(),
                    "exported": s.exported,
                    "start": { "line": start.line, "col": start.col },
                    "end": { "line": end.line, "col": end.col },
                    "name_span_bytes": name_span.clone(),
                    "full_span_bytes": full_span.clone(),
                    "name_span": name_span,
                    "full_span": full_span,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        let root = workspace_root(cli);
        for symbol in &selected {
            println!(
                "{:<12} {:<30} {}",
                symbol.kind.as_str(),
                symbol.qualified_name(),
                shown_path(&root, &symbol.file)
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
        // Resolved names, never raw ids.
        let place = |id: crate::model::SymbolId| {
            index.symbol(id).map(|s| {
                let source = crate::vfs::read_to_string(&s.file).unwrap_or_default();
                let at = crate::span::LineIndex::new(&source).line_col(s.name_span.start, &source);
                serde_json::json!({
                    "symbol": s.qualified_name(),
                    "file": s.file,
                    "line": at.line,
                    "col": at.col,
                })
            })
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": symbol.qualified_name(),
                "name": declared.name,
                "declared": declared.declared,
                "inferred": declared.inferred.as_ref().map(|i| serde_json::json!({
                    "type": i.ty,
                    "basis": i.basis.describe(),
                    "evidence": i.from.and_then(place),
                })),
                "parameters": declared.parameters,
                "defined_at": declared.defined_at.and_then(place),
            }))?
        );
        return Ok(());
    }

    let root = workspace_root(cli);
    println!("{}  {}", declared.name, declared.describe());
    if let Some(inferred) = &declared.inferred {
        if let Some(from) = inferred.from.and_then(|id| index.symbol(id)) {
            let source = crate::vfs::read_to_string(&from.file).unwrap_or_default();
            let at = crate::span::LineIndex::new(&source).line_col(from.name_span.start, &source);
            println!(
                "  evidence: {} at {}:{}",
                from.name,
                shown_path(&root, &from.file),
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
            shown_path(&root, &defined.file),
            at
        );
    }
    match (&declared.declared, &declared.inferred) {
        (None, None) => println!(
            "\nThe source wrote no type here, and nothing follows from what it did \n\
             write. That is the answer and not a gap in one."
        ),
        (None, Some(_)) => println!(
            "\nThe source wrote no type here. The above was worked out from the \n\
             evidence named, and is a derivation and not a contract."
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
                    "role": d.role.label(),
                    "file": d.location.file,
                    "line": d.location.line,
                    "col": d.location.col,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let root = workspace_root(cli);
    for definition in &found.definitions {
        if first_only && definition.role != navigate::DefinitionRole::Primary {
            continue;
        }
        println!(
            "{:<18} {:<12} {}:{}:{}",
            definition.role.label(),
            definition.kind.as_str(),
            shown_path(&root, &definition.location.file),
            definition.location.line,
            definition.location.col
        );
        if !definition.location.preview.is_empty() {
            println!("                   {}", definition.location.preview);
        }
    }

    if found.is_polymorphic() && !first_only {
        println!(
            "\n`{}` sits on an abstraction, so which one runs is a runtime fact. \n\
             This lists every implementation.",
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

    let root = workspace_root(cli);
    for (name, file) in &rendered {
        println!("{:<34} {}", name, shown_path(&root, file));
    }
    println!(
        "\n{} implementation(s) that a call through this declaration could reach, \n\
         found by reading which types declare that they implement it. The program \n\
         chooses one of them while it runs, and this list cannot say which.",
        rendered.len()
    );
    Ok(())
}

fn cmd_usages(cli: &Cli, target: &str, include_unresolved: bool) -> Result<()> {
    use crate::navigate;

    let index = build_index(cli, &[])?;
    let symbol = resolve_target(cli, &index, target)?;
    let found = navigate::usages_of(&index, symbol.id);
    // The definition sites, kept apart from the uses.
    let defined = navigate::definitions_of(&index, symbol.id);

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
        let definitions: Vec<_> = defined
            .definitions
            .iter()
            .map(|d| {
                serde_json::json!({
                    "file": d.location.file,
                    "line": d.location.line,
                    "col": d.location.col,
                    "role": d.role,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "symbol": found.query,
                "definitions": definitions,
                "usages": render(&all),
                "same_name_elsewhere": if include_unresolved { render(&weak) } else { Vec::new() },
                "skipped_files": skipped_files_json(&index),
                "unparsed_files": unparsed_files_json(&index),
            }))?
        );
        return Ok(());
    }

    let root = workspace_root(cli);
    for (file, usages) in found.by_file() {
        println!("{}", shown_path(&root, file));
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

    println!(
        "\n{} definition site(s), not counted among the uses:",
        defined.definitions.len()
    );
    for definition in &defined.definitions {
        println!(
            "  {}:{}:{}  {}",
            shown_path(&root, &definition.location.file),
            definition.location.line,
            definition.location.col,
            definition.role.label()
        );
    }

    if include_unresolved && !found.same_name_elsewhere.is_empty() {
        println!(
            "\n{} occurrence(s) of the same name that did NOT resolve here:",
            found.same_name_elsewhere.len()
        );
        for usage in found.same_name_elsewhere.iter().take(20) {
            println!(
                "  {}:{}:{}  [{}]",
                shown_path(&root, &usage.location.file),
                usage.location.line,
                usage.location.col,
                usage.confidence.as_str()
            );
        }
        if found.same_name_elsewhere.len() > 20 {
            println!("  … and {} more", found.same_name_elsewhere.len() - 20);
        }
    }

    if !found.in_text.is_empty() {
        println!(
            "\n{} mention(s) of the name, matched as text. Nothing links these \
             to the declaration, so no command rewrites them:",
            found.in_text.len()
        );
        for usage in found.in_text.iter().take(20) {
            println!(
                "  {}:{}:{}  {}",
                shown_path(&root, &usage.location.file),
                usage.location.line,
                usage.location.col,
                usage.location.preview
            );
        }
        if found.in_text.len() > 20 {
            println!("  … and {} more", found.in_text.len() - 20);
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

    let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut locate = |file: &PathBuf, offset: usize| -> crate::span::LineCol {
        let source = sources
            .entry(file.clone())
            .or_insert_with(|| crate::vfs::read_to_string(file).unwrap_or_default());
        LineIndex::new(source).line_col(offset, source)
    };

    if cli.json {
        // What a rename would rewrite.
        let rewritable = crate::refactor::rename::rewritable_spans(&index, symbol.id);
        let render =
            |list: &[&crate::model::Reference],
             locate: &mut dyn FnMut(&PathBuf, usize) -> crate::span::LineCol| {
                list.iter()
                    .map(|r| {
                        let at = locate(&r.file, r.span.start);
                        serde_json::json!({
                            "file": r.file,
                            "line": at.line,
                            "col": at.col,
                            "kind": format!("{:?}", r.kind).to_lowercase(),
                            "confidence": r.confidence.as_str(),
                            "rewritable": rewritable.contains(&(r.file.clone(), r.span)),
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
                "skipped_files": skipped_files_json(&index),
                "unparsed_files": unparsed_files_json(&index),
            }))?
        );
        return Ok(());
    }

    let root = workspace_root(cli);
    println!("{} reference(s) to {}", refs.len(), symbol.qualified_name());
    for r in &refs {
        let at = locate(&r.file, r.span.start);
        println!(
            "  {}:{}:{}  [{}]",
            shown_path(&root, &r.file),
            at.line,
            at.col,
            r.confidence.as_str()
        );
    }

    if include_unresolved && !weak.is_empty() {
        println!(
            "\n{} occurrence(s) of the same name that did NOT resolve here:",
            weak.len()
        );
        for r in &weak {
            let at = locate(&r.file, r.span.start);
            println!(
                "  {}:{}:{}  [{}]",
                shown_path(&root, &r.file),
                at.line,
                at.col,
                r.confidence.as_str()
            );
        }
    }
    Ok(())
}

/// Resolve `--lang` values, failing loudly on an unknown name and not silently
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

fn scan_options(cli: &Cli, names: &[String]) -> Result<ScanOptions> {
    let mut options = ScanOptions {
        languages: resolve_languages(names)?,
        ..Default::default()
    };
    if let Some(bytes) = cli.max_file_size {
        options.max_file_bytes = bytes;
    }
    options.respect_ignore = !cli.no_ignore;
    Ok(options)
}

fn cmd_scan(cli: &Cli, languages: &[String]) -> Result<()> {
    let options = scan_options(cli, languages)?;
    let result = scan(&cli.root, &options)?;

    if cli.json {
        let files: Vec<_> = result
            .files
            .iter()
            .map(|f| {
                serde_json::json!({
                    "path": f.path,
                    // The absolute spelling, because every other command's JSON says `file`
                    // absolutely and a reader joining the two needs one key that matches
                    // without path arithmetic.
                    "file": f.path.canonicalize().unwrap_or_else(|_| f.path.clone()),
                    "language": f.language.name(),
                })
            })
            .collect();
        let too_large: Vec<_> = result
            .skipped_too_large
            .iter()
            .map(|(p, size)| serde_json::json!({ "path": p, "bytes": size }))
            .collect();
        let skipped: Vec<_> = result
            .skipped_symlinks
            .iter()
            .map(|(p, reason)| serde_json::json!({ "path": p, "reason": reason }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "files": files,
                "skipped": skipped,
                "skipped_too_large": too_large,
                "unsupported": result.unsupported,
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
    let options = scan_options(cli, languages)?;
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
    // The count and where.
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
                        "file": p.canonicalize().unwrap_or_else(|_| p.clone()),
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
            // Every position, up to a handful.
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
    if !result.skipped_symlinks.is_empty() {
        println!(
            "\n{} symlink(s) skipped; this reads each file where it really lives:",
            result.skipped_symlinks.len()
        );
        for (path, reason) in &result.skipped_symlinks {
            println!("  {} ({reason})", path.display());
        }
    }
    if !result.unsupported.is_empty() {
        let total: usize = result.unsupported.values().sum();
        // Commonest first, and only the leading few.
        let mut kinds: Vec<_> = result.unsupported.iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let named: Vec<String> = kinds
            .iter()
            .take(5)
            .map(|(kind, count)| format!("{kind} ({count})"))
            .collect();
        let rest = match kinds.len().saturating_sub(5) {
            0 => String::new(),
            more => format!(", and {more} other kind(s)"),
        };
        println!(
            "\n{total} file(s) in no language this reads: {}{rest}. \
             `fr capabilities` lists what it does read.",
            named.join(", ")
        );
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

    #[test]
    fn the_machine_progress_line_is_json_with_both_counts() {
        let line = indexing_progress_line(3, 10);
        let parsed: serde_json::Value = serde_json::from_str(&line).expect("one JSON line");
        assert_eq!(parsed["indexing"]["done"], 3);
        assert_eq!(parsed["indexing"]["total"], 10);
    }

    #[test]
    fn a_file_changed_after_indexing_is_named_as_the_race() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.py");
        let first = "def f():\n    return 1\n";
        crate::vfs::write(&path, first).unwrap();
        let index =
            Index::build_from_sources(&[(path.clone(), Language::Python, first.to_string())])
                .unwrap();

        // Nothing moved yet: the plan is fresh and passes.
        let mut edits = crate::edit::EditSet::new();
        edits.add(
            &path,
            crate::edit::Edit::new(crate::span::Span::new(4, 5), "g", "rename"),
        );
        refuse_stale_plan(&index, &edits).expect("a fresh plan is accepted");

        crate::vfs::write(&path, "def f():\n    return 2\n").unwrap();
        let err = refuse_stale_plan(&index, &edits).unwrap_err().to_string();
        assert!(
            err.contains("changed after the plan read it"),
            "the refusal names the race, instead of a syntax error: {err}"
        );
    }
}
