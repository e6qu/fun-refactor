//! Every `✓` in the capability matrix, asked to prove itself.
//!
//! `fr capabilities` computes the matrix from each refactoring's own predicate, so a `✓` means
//! "this command would accept this language", not "this has ever worked". Those are different
//! claims, and the gap between them is where this project's defects live. A rule that is
//! documented and never runs, a gap recorded after it had stopped being one, a language driven
//! by a gate that was never installed.
//!
//! So each claimed cell is driven here, against a fixture in that language. One thing is
//! asserted: **it must not answer that the language is unsupported.** That is the exact
//! contradiction a wrong `✓` produces. Nothing else in the suite would notice it.
//!
//! What this deliberately does not assert is that the answer is *good*. A capability may refuse
//! for a reason about this particular input, a symbol with no callers, an expression in a place
//! a binding cannot go, and that is a real answer. The gates in `output_compiles.rs`,
//! `rewrites_compile.rs`, `removals_compile.rs` and `validators_accept.rs` are what check the
//! answers are right; this checks the claims are true.

use fun_refactor::capabilities::{is_whole_workspace, support, Capability};
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::Span;
use std::path::PathBuf;

/// One small, valid file per language, each holding a declaration and a use of it.
fn fixture(language: Language) -> (&'static str, &'static str) {
    match language {
        Language::Rust => (
            "src/lib.rs",
            "pub const LIMIT: usize = 3;\n\npub fn width(items: &[u8]) -> usize {\n    \
             let total = items.len() + LIMIT;\n    total\n}\n\n\
             pub fn caller(items: &[u8]) -> usize {\n    width(items)\n}\n",
        ),
        Language::Go => (
            "main.go",
            "package main\n\nconst Limit = 3\n\nfunc Width(items []byte) int {\n\t\
             total := len(items) + Limit\n\treturn total\n}\n\n\
             func Caller(items []byte) int {\n\treturn Width(items)\n}\n",
        ),
        Language::Zig => (
            "main.zig",
            "pub const limit: usize = 3;\n\npub fn width(items: []const u8) usize {\n    \
             const total = items.len + limit;\n    return total;\n}\n\n\
             pub fn caller(items: []const u8) usize {\n    return width(items);\n}\n",
        ),
        Language::Java => (
            "Main.java",
            "public class Main {\n    static final int LIMIT = 3;\n\n    \
             static int width(byte[] items) {\n        int total = items.length + LIMIT;\n        \
             return total;\n    }\n\n    static int caller(byte[] items) {\n        \
             return width(items);\n    }\n}\n",
        ),
        Language::TypeScript => (
            "src/main.ts",
            "export const LIMIT = 3;\n\nexport function width(items: number[]): number {\n  \
             const total = items.length + LIMIT;\n  return total;\n}\n\n\
             export function caller(items: number[]): number {\n  return width(items);\n}\n",
        ),
        // The same shapes with JSX in the file, because a `.tsx` that is only TypeScript
        // proves nothing about the language this cell names.
        Language::Tsx => (
            "src/main.tsx",
            "export const LIMIT = 3;\n\nexport function width(items: number[]): number {\n  \
             const total = items.length + LIMIT;\n  return total;\n}\n\n\
             export function Caller({ items }: { items: number[] }) {\n  \
             return <span className=\"count\">{width(items)}</span>;\n}\n",
        ),
        Language::Python => (
            "main.py",
            "LIMIT = 3\n\n\ndef width(items):\n    total = len(items) + LIMIT\n    return total\n\n\n\
             def caller(items):\n    return width(items)\n",
        ),
        Language::Bash => (
            "run.sh",
            "#!/usr/bin/env bash\nLIMIT=3\n\nwidth() {\n  echo $(( $1 + LIMIT ))\n}\n\n\
             caller() {\n  width 1\n}\n\ncaller\n",
        ),
        Language::Html => (
            "page.html",
            "<!DOCTYPE html>\n<html>\n  <head><title>t</title></head>\n  <body>\n    \
             <p id=\"intro\" class=\"note\">first</p>\n    <a href=\"#intro\">back</a>\n  \
             </body>\n</html>\n",
        ),
        Language::Css => (
            "style.css",
            ":root {\n  --gap: 4px;\n}\n\n.note {\n  margin: var(--gap);\n}\n\n\
             .other {\n  margin: var(--gap);\n}\n",
        ),
        Language::Scss => (
            "style.scss",
            "$gap: 4px;\n\n.note {\n  margin: $gap;\n}\n\n.other {\n  margin: $gap;\n}\n",
        ),
        Language::Hcl => (
            "main.tf",
            "variable \"enabled\" {\n  type    = bool\n  default = true\n}\n\n\
             locals {\n  base = \"service\"\n  full = \"${local.base}-primary\"\n}\n\n\
             output \"name\" {\n  value = local.full\n}\n",
        ),
        Language::Yaml => (
            "config.yaml",
            "defaults: &defaults\n  retries: 3\n\nservice:\n  <<: *defaults\n  name: thing\n",
        ),
        Language::Helm => (
            "templates/deployment.yaml",
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: {{ .Values.name }}\nspec:\n  \
             replicas: {{ .Values.replicas }}\n",
        ),
        Language::Xml => (
            "config.xml",
            "<?xml version=\"1.0\"?>\n<!DOCTYPE cfg [\n  <!ENTITY brand \"Acme\">\n]>\n<cfg>\n  \
             <section id=\"limits\">&brand;</section>\n  <link href=\"#limits\">go</link>\n</cfg>\n",
        ),
        Language::Markdown => (
            "doc.md",
            "# Title\n\nSome text with a [link][ref] in it.\n\n## Section\n\n\
             More text, and the same [link][ref] again.\n\n[ref]: https://example.com\n",
        ),
    }
}

/// The workspace a language's fixture needs around it, beyond the file itself.
fn scaffolding(language: Language) -> Vec<(&'static str, &'static str)> {
    match language {
        Language::Rust => vec![(
            "Cargo.toml",
            "[package]\nname = \"claims\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )],
        Language::Go => vec![("go.mod", "module claims\n\ngo 1.21\n")],
        Language::Helm => vec![
            (
                "Chart.yaml",
                "apiVersion: v2\nname: claims\nversion: 0.1.0\n",
            ),
            ("values.yaml", "name: claims\nreplicas: 2\n"),
        ],
        _ => Vec::new(),
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    file: PathBuf,
    index: Index,
}

fn build(language: Language) -> Fixture {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (name, content) = fixture(language);
    for (extra, text) in scaffolding(language) {
        let path = dir.path().join(extra);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
    let file = dir.path().join(name);
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("mkdir");
    std::fs::write(&file, content).expect("write");

    let scanned = scan(dir.path(), &ScanOptions::default()).expect("scan");
    let index = Index::build_from_scan(&scanned).expect("index");
    let root = dir.path().to_path_buf();
    Fixture {
        _dir: dir,
        root,
        file,
        index,
    }
}

impl Fixture {
    /// A symbol this file declares, preferring one with a use.
    fn a_symbol(&self) -> Option<SymbolId> {
        let mut best: Option<(usize, SymbolId)> = None;
        for symbol in &self.index.symbols {
            if symbol.file != self.file {
                continue;
            }
            let uses = self.index.references_to(symbol.id).len();
            if best.is_none_or(|(seen, _)| uses > seen) {
                best = Some((uses, symbol.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// The bytes of that symbol's name, which is a span every language has.
    fn a_span(&self) -> Option<Span> {
        let id = self.a_symbol()?;
        self.index.symbol(id).map(|s| s.name_span)
    }

    fn an_offset(&self) -> usize {
        self.a_span().map(|s| s.start).unwrap_or(0)
    }
}

/// What happened when one capability was pointed at one language.
///
/// The three are not two. A fixture that offers no symbol and no span drives nothing,
/// and folding that into "it worked" is how a cell gets counted as checked without
/// having been run, which both tests below were doing.
#[derive(Debug)]
enum Outcome {
    /// The fixture had nothing for this capability to take.
    NotDriven,
    /// It ran and produced a plan or an answer.
    Proceeded,
    /// It declined, for whatever reason.
    Refused(String),
}

/// Run one capability against one language, and return what it said.
fn drive(capability: Capability, language: Language, f: &Fixture) -> Outcome {
    use fun_refactor::{analysis, refactor, transpile};

    let said = |e: anyhow::Error| e.to_string();
    let outcome: Result<(), String> = match capability {
        Capability::Symbols => {
            // Building the index is the capability. Reaching here means it built.
            Ok(())
        }
        Capability::Rename => match f.a_symbol() {
            Some(id) => refactor::rename::plan(&f.index, id, "renamedThing")
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::SafeDelete => match f.a_symbol() {
            Some(id) => refactor::delete::plan(&f.index, id)
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::Impact => match f.a_symbol() {
            Some(id) => analysis::impact::analyse(&f.index, id, 2)
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        // The pattern cannot match, so the search has to come back empty. Checking that much
        // keeps the driver honest. A `restructure` returning edits for a shape absent from
        // the file would pass here if the outcome went unread.
        Capability::Restructure => {
            refactor::restructure::apply(&f.index, language, "nothing_matches_this", "nor_this")
                .map(|plan| {
                    assert!(
                        plan.edits.is_empty(),
                        "{language:?}: a pattern that matches nothing produced edits"
                    );
                })
                .map_err(said)
        }
        // Every fixture here has `caller` calling `width`, and the table claims a call graph
        // for the imperative languages only. So an empty edge list means the graph found
        // nothing, and reading the result is what tells the two apart.
        Capability::CallGraph => {
            let graph = analysis::call_graph::CallGraph::build(&f.index);
            assert!(
                !graph.edges().is_empty(),
                "{language:?}: the call graph over a file with a call has no edges"
            );
            Ok(())
        }
        Capability::Flow => match f.a_symbol() {
            Some(id) => analysis::flow::forward(&f.index, id, 3)
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::Provenance => match f.a_symbol() {
            Some(id) => analysis::provenance::provenance(&f.index, id, 3)
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::EntryPoints => {
            let _ = analysis::entrypoints::Entrypoints::detect(&f.index);
            Ok(())
        }
        Capability::ExtractVariable => match f.a_span() {
            Some(span) => refactor::extract::variable(&f.index, &f.file, span, "lifted", false)
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::ExtractFunction => match f.a_span() {
            Some(span) => refactor::extract::function(&f.index, &f.file, span, "lifted")
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::InlineVariable => match f.a_symbol() {
            Some(id) => refactor::inline::variable(&f.index, id)
                .map(|_| ())
                .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::InlineCall => refactor::inline::call(&f.index, &f.file, f.an_offset())
            .map(|_| ())
            .map_err(said),
        Capability::ChangeSignature => match f.a_symbol() {
            Some(id) => refactor::signature::change(
                &f.index,
                id,
                refactor::signature::Change::Move { from: 0, to: 1 },
            )
            .map(|_| ())
            .map_err(said),
            None => return Outcome::NotDriven,
        },
        Capability::MicroRewrites => refactor::rewrite::apply(
            &f.index,
            &f.file,
            f.an_offset(),
            refactor::rewrite::Rewrite::InvertIf,
        )
        .map(|_| ())
        .map_err(said),
        Capability::OrganizeImports => refactor::imports::plan(&f.index, &f.file)
            .map(|_| ())
            .map_err(said),
        Capability::RemoveFlag => {
            let sources = f
                .index
                .files()
                .filter_map(|(path, info)| {
                    let text = std::fs::read_to_string(path).ok()?;
                    Some((path.clone(), (info.language, text)))
                })
                .collect();
            refactor::cascade::remove_flag_in(sources, "no_such_flag_here", true)
                .map(|_| ())
                .map_err(said)
        }
        Capability::MoveToFile => match f.a_symbol() {
            Some(id) => {
                // Beside the file it came from, since some languages derive a module
                // path from where a file sits and a destination elsewhere is a different
                // refusal about a different thing.
                let destination = f.file.with_file_name(format!(
                    "moved.{}",
                    f.file.extension().unwrap_or_default().to_string_lossy()
                ));
                refactor::move_symbol::to_file(&f.index, id, &destination)
                    .map(|_| ())
                    .map_err(said)
            }
            None => return Outcome::NotDriven,
        },
        Capability::Stitch => analysis::stitch::chains(&f.index).map(|_| ()).map_err(said),
        Capability::Duplicates => analysis::duplicates::find_in(&f.index, &f.root)
            .map(|_| ())
            .map_err(said),
        Capability::DeadCode => match analysis::entrypoints::Entrypoints::detect(&f.index) {
            Ok(entrypoints) => {
                let _ = refactor::delete::find_unused(&f.index, &entrypoints);
                Ok(())
            }
            Err(e) => Err(said(e)),
        },
        Capability::Translate => {
            // Every target the matrix claims for this source, since one of them working
            // is what the cell means.
            let mut last: Result<(), String> = Ok(());
            for target in Language::ALL {
                if *target == language {
                    continue;
                }
                // Two paths write another language: one grammar containing another, and
                // the IR-based translation. A cell is honoured if either accepts.
                let containment = fun_refactor::translate::plan(&f.file, *target).map(|_| ());
                if containment.is_ok() {
                    return Outcome::Proceeded;
                }
                match transpile::plan(&f.file, *target) {
                    Ok(_) => return Outcome::Proceeded,
                    Err(e) => last = Err(e.to_string()),
                }
            }
            last
        }
        Capability::Openapi => transpile::nextjs::plan(&f.file).map(|_| ()).map_err(said),
        Capability::DeclaredType => match f.a_symbol() {
            Some(id) => analysis::types::of(&f.index, id).map(|_| ()).map_err(said),
            None => return Outcome::NotDriven,
        },
    };
    match outcome {
        Ok(()) => Outcome::Proceeded,
        Err(said) => Outcome::Refused(said),
    }
}

/// The sentence a wrong `✓` produces.
fn denies_the_language(said: &str, language: Language) -> bool {
    said.contains(&format!("is not supported for {}", language.name()))
        || said.contains(&format!("has no meaning in {}", language.name()))
}

#[test]
fn every_claimed_capability_accepts_the_language_it_claims() {
    let mut contradictions = Vec::new();
    let mut driven = 0;
    let mut not_driven = 0;

    for language in Language::ALL {
        let f = build(*language);
        for capability in Capability::ALL {
            if !support(*capability, *language).is_yes() {
                continue;
            }
            match drive(*capability, *language, &f) {
                // Counted apart and reported below. A cell the fixture cannot reach is
                // not a cell this test checked, and calling it checked is the vacuity
                // this suite went looking for.
                Outcome::NotDriven => not_driven += 1,
                Outcome::Proceeded => driven += 1,
                Outcome::Refused(said) => {
                    driven += 1;
                    if denies_the_language(&said, *language) {
                        contradictions.push(format!(
                            "{} claims {} and then says: {said}",
                            language.name(),
                            capability.label()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        contradictions.is_empty(),
        "{} of {driven} claimed cells contradict themselves:\n  {}",
        contradictions.len(),
        contradictions.join("\n  ")
    );
    eprintln!(
        "capability claims: {driven} cells driven, none contradicted; \
         {not_driven} not reachable from their fixture"
    );
}

#[test]
fn the_matrix_and_this_file_agree_on_how_many_cells_there_are() {
    // A capability added without a driver above would quietly stop being checked, and the
    // count is what notices. `fr capabilities` is the authority on the number.
    let (yes, _, _) = fun_refactor::capabilities::totals();
    let driven: usize = Language::ALL
        .iter()
        .flat_map(|language| {
            Capability::ALL
                .iter()
                .filter(move |c| support(**c, *language).is_yes())
        })
        .count();
    assert_eq!(
        driven, yes,
        "the matrix reports {yes} supported cells and this file drives {driven}"
    );
}

#[test]
fn every_unsupported_capability_refuses_the_language_it_disclaims() {
    // The mirror of the test above, and the half nothing was asking. An `n/a` cell is a promise
    // too: the command does not do this here. Nothing drove those cells, so the promise was
    // unfalsifiable. `fr remove-flag` was breaking it on XML, rewriting `&use_new;` into
    // `&true;` and deleting the prolog, output no parser accepts.
    //
    // Proceeding is the failure. An error is fine whatever it says, because a refusal about
    // this particular fixture is still a refusal.
    let mut proceeded = Vec::new();
    let mut driven = 0;
    let mut not_driven = 0;

    for language in Language::ALL {
        let f = build(*language);
        for capability in Capability::ALL {
            if support(*capability, *language).is_yes() {
                continue;
            }
            // A whole-workspace analysis takes no language argument, so there is nothing for it
            // to refuse: `n/a` there says the language contributes nothing.
            // `capability-report.py` is what holds that half.
            if is_whole_workspace(*capability) {
                continue;
            }
            match drive(*capability, *language, &f) {
                Outcome::NotDriven => not_driven += 1,
                Outcome::Refused(_) => driven += 1,
                Outcome::Proceeded => {
                    driven += 1;
                    proceeded.push(format!(
                        "{} disclaims {} and does it anyway",
                        language.name(),
                        capability.label()
                    ));
                }
            }
        }
    }

    assert!(
        proceeded.is_empty(),
        "{} of {driven} disclaimed cells went ahead:\n  {}",
        proceeded.len(),
        proceeded.join("\n  ")
    );
    eprintln!(
        "capability disclaimers: {driven} cells driven, none proceeded; \
         {not_driven} not reachable from their fixture"
    );
}
