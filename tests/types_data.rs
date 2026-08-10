//! The data behind the types tutorial, produced by running the tool over its stages.
//!
//! The page shows code, and beside each symbol what the tool says about it: the type,
//! where it is defined, what calls it. None of that is written by hand — it is asked of
//! the same index `fr type`, `fr def` and `fr callers` ask, so the panel a reader clicks
//! is the tool's answer and not a transcription of one.
//!
//! Regenerate with `UPDATE_SITE_DATA=1 cargo test --test types_data`.

use fun_refactor::analysis::call_graph::CallGraph;
use fun_refactor::analysis::types;
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolKind;
use fun_refactor::scan::ScanOptions;
use fun_refactor::span::LineIndex;
use std::path::{Path, PathBuf};

/// Every stage, with what the page says about it.
///
/// The prose lives here and not in the HTML because each stage's claim is about the
/// code beside it, and the two drifting apart is the failure this whole arrangement
/// exists to prevent.
const STAGES: &[(&str, &str, &str, &str)] = &[
    (
        "stage0_as_found",
        "The code as found",
        "Nothing is written down. Every value is a string, a number or a dictionary, and \
         the program has no way to tell one from another.",
        "",
    ),
    (
        "stage1_annotated",
        "Annotate what is already true",
        "The same code, with the types it already had. No behaviour changes and almost \
         no bug becomes impossible — but the checker can see the program now, and every \
         later stage is a change to something it can check.",
        "Nothing yet. This is the stage people stop at.",
    ),
    (
        "stage2_named_ids",
        "Name the primitives",
        "A payment id, a customer id and a vendor id are all strings, and a string fits \
         wherever a string goes. Naming each one makes them stop fitting.",
        "Passing a customer id where a payment id belongs.",
    ),
    (
        "stage3_closed_sets",
        "Close the string sets",
        "Three providers, three vocabularies, one domain. Each provider's words are \
         translated at the door and never travel further; a word this provider does not \
         use is not a state, it is unread input.",
        "A typo'd status, an unknown status, and a provider's vocabulary leaking into \
         code that should not know which provider it is.",
    ),
    (
        "stage4_grouped",
        "Group what travels together",
        "A payment is not a dictionary that happens to have an amount in it. What always \
         travels together becomes one thing, and a missing field becomes a missing field \
         instead of a `KeyError` at three in the morning.",
        "A dictionary missing a key, and a dictionary carrying a key nothing reads.",
    ),
    (
        "stage5_unconstructible",
        "Make the invalid unconstructible",
        "`Money` is a whole number of the currency's smallest unit and the currency it is \
         in. It is built through one function, which is the one place a negative amount \
         can be turned away — and two currencies are never one number.",
        "A negative amount, dollars added to cents, and dollars added to euros.",
    ),
    (
        "stage6_state_machine",
        "Make the state machine a type",
        "Each state is its own type carrying exactly what that state has: a capture has a \
         capture time, a failure has a reason, and neither has the other. `capture` takes \
         an `Authorized` and a `PayoutEnabledVendor`, and there is no way to obtain \
         either except by passing the check that produces it.",
        "Refunding before capture, capturing twice, capturing to an unverified vendor, \
         and a failed payment carrying a capture time.",
    ),
    (
        "stage7_deleted",
        "Delete what can no longer happen",
        "The checks are unreachable now. Deleting them is the proof: not that a type was \
         added, but that the code which existed only to guard against the impossible is \
         gone and nothing broke.",
        "The defences themselves.",
    ),
];

fn tutorial() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/types_tutorial")
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// One clickable symbol: where it is on the page, and everything the tool says about it.
struct Marked {
    line: usize,
    col: usize,
    len: usize,
    name: String,
    kind: String,
    /// The type, however the tool arrived at it.
    ty: String,
    /// Whether that came from the source or from a derivation, in the tool's words.
    origin: String,
    defined: String,
    callers: Vec<String>,
}

fn marks(index: &Index, file: &Path, source: &str) -> Vec<Marked> {
    let lines = LineIndex::new(source);
    let graph = CallGraph::build(index);
    let mut out = Vec::new();

    let Some(info) = index.file(file) else {
        return out;
    };
    for id in &info.symbols {
        let Some(symbol) = index.symbol(*id) else {
            continue;
        };
        let answer = types::of(index, *id).expect("a type answer");
        let (ty, origin) = match (&answer.declared, &answer.inferred) {
            (Some(ty), _) => (ty.clone(), "the source wrote it".to_string()),
            (None, Some(inferred)) => (inferred.ty.clone(), inferred.basis.describe().to_string()),
            (None, None) => (
                "not known".to_string(),
                "the source wrote nothing, and nothing follows from what it did".to_string(),
            ),
        };
        let defined = answer
            .defined_at
            .and_then(|t| index.symbol(t))
            .map(|t| {
                let text = std::fs::read_to_string(&t.file).unwrap_or_default();
                let at = LineIndex::new(&text).line_col(t.name_span.start, &text);
                format!("{}:{}", file_name(&t.file), at)
            })
            .unwrap_or_default();
        let callers: Vec<String> = match symbol.kind.is_callable() {
            true => graph
                .callers(*id)
                .into_iter()
                .filter_map(|(from, _)| index.symbol(from).map(|s| s.qualified_name()))
                .collect(),
            false => Vec::new(),
        };

        // The definition, and every use of it, so a click anywhere lands somewhere.
        let mut spans = vec![symbol.name_span];
        spans.extend(
            index
                .references_to(*id)
                .into_iter()
                .filter(|r| r.file == *file)
                .map(|r| r.span),
        );
        for span in spans {
            let at = lines.line_col(span.start, source);
            out.push(Marked {
                line: at.line,
                col: at.col,
                len: span.end - span.start,
                name: symbol.name.clone(),
                kind: symbol.kind.as_str().to_string(),
                ty: ty.clone(),
                origin: origin.clone(),
                defined: defined.clone(),
                callers: callers.clone(),
            });
        }
    }
    out.sort_by_key(|m| (m.line, m.col));
    out.dedup_by_key(|m| (m.line, m.col));
    out
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// How much of each stage the tool can account for, which is the page's own scoreboard.
fn scoreboard(index: &Index, language: Language) -> (usize, usize, usize) {
    let (mut declared, mut inferred, mut unknown) = (0, 0, 0);
    for symbol in &index.symbols {
        if symbol.language != language
            || !matches!(
                symbol.kind,
                SymbolKind::Variable
                    | SymbolKind::Constant
                    | SymbolKind::Parameter
                    | SymbolKind::Field
            )
        {
            continue;
        }
        let answer = types::of(index, symbol.id).expect("a type answer");
        match (&answer.declared, &answer.inferred) {
            (Some(_), _) => declared += 1,
            (None, Some(_)) => inferred += 1,
            (None, None) => unknown += 1,
        }
    }
    (declared, inferred, unknown)
}

fn render_file(index: &Index, stage: &Path, name: &str) -> String {
    let path = stage.join(name);
    let source = std::fs::read_to_string(&path).expect("a stage file");
    let mut out = format!(
        "      {{ path: \"{name}\", code: \"{}\", marks: [\n",
        escape(&source)
    );
    for mark in marks(index, &path, &source) {
        out.push_str(&format!(
            "        {{ line: {}, col: {}, len: {}, name: \"{}\", kind: \"{}\", \
             type: \"{}\", origin: \"{}\", defined: \"{}\", callers: [{}] }},\n",
            mark.line,
            mark.col,
            mark.len,
            escape(&mark.name),
            mark.kind,
            escape(&mark.ty),
            escape(&mark.origin),
            escape(&mark.defined),
            mark.callers
                .iter()
                .map(|c| format!("\"{}\"", escape(c)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push_str("      ] },\n");
    out
}

fn types_data() -> String {
    let mut out = String::from(
        "// Generated by `cargo test --test types_data`. Do not edit.\n\
         //\n\
         // Every type, definition and caller below is what the tool answered for the\n\
         // code beside it. Regenerate with:\n\
         //   UPDATE_SITE_DATA=1 cargo test --test types_data\n\
         export const STAGES = [\n",
    );
    for (dir, title, lede, kills) in STAGES {
        let stage = tutorial().join(dir);
        let index = Index::build(&stage, &ScanOptions::default()).expect("an index");
        let (pd, pi, pu) = scoreboard(&index, Language::Python);
        let (td, ti, tu) = scoreboard(&index, Language::TypeScript);
        out.push_str(&format!(
            "  {{\n    id: \"{dir}\",\n    title: \"{}\",\n    lede: \"{}\",\n    \
             kills: \"{}\",\n    scoreboard: {{ python: [{pd}, {pi}, {pu}], \
             typescript: [{td}, {ti}, {tu}] }},\n    files: [\n",
            escape(title),
            escape(lede),
            escape(kills)
        ));
        out.push_str(&render_file(&index, &stage, "payments.py"));
        out.push_str(&render_file(&index, &stage, "payments.ts"));
        out.push_str("    ],\n  },\n");
    }
    out.push_str("];\n");
    out
}

#[test]
fn the_types_page_shows_what_the_tool_answers() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/types-data.js");
    let generated = types_data();
    if std::env::var("UPDATE_SITE_DATA").is_ok() {
        std::fs::write(&path, &generated).expect("writing the generated data");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "docs/types-data.js is not what the tool answers any more. Regenerate it:\n\n    \
         UPDATE_SITE_DATA=1 cargo test --test types_data\n"
    );
}
