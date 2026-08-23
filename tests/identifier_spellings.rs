//! One spelling per thing.
//!
//! Several enums here have both a serde representation and an `as_str`. Where that `as_str` is
//! an *identifier*, what `--json` emits, what a catalogue matches on, what a person types, the
//! two must be the same string. Three of `SymbolKind`'s twenty-one variants disagreed: the tool
//! wrote `"kind": "type"` and could not read it back. So its own output was not valid input to
//! itself, and nothing was checking.
//!
//! Not every `as_str` is an identifier. `Capability` and `Basis` return prose for a reader,
//! "call graph", "from the literal", and are named `label` and `describe` so the difference is
//! visible and not remembered.
//!
//! The spellings are read out of the `as_str` match in the source and not listed here. That
//! match is exhaustive, so the compiler already forces a new variant to appear in it. Reading
//! it means a new variant is covered by this test the day it is added. A list written here
//! would be one more thing to forget.

use fun_refactor::lang::Language;
use fun_refactor::model::{Confidence, SymbolKind};
use std::path::Path;

/// Every `Enum::Variant => "spelling"` arm of the named function.
fn spellings(file: &str, enum_name: &str, function: &str) -> Vec<String> {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
        .unwrap_or_else(|e| panic!("{file}: {e}"));
    let body = source
        .split_once(&format!("impl {enum_name} {{"))
        .unwrap_or_else(|| panic!("no `impl {enum_name}` in {file}"))
        .1
        .split_once(&format!("fn {function}"))
        .unwrap_or_else(|| panic!("no `{function}` on {enum_name}"))
        .1;
    let body = &body[..body.find("\n    }").unwrap_or(body.len())];

    let mut found = Vec::new();
    for arm in body.split(&format!("{enum_name}::")).skip(1) {
        let Some((_, rest)) = arm.split_once("=> \"") else {
            continue;
        };
        let Some((spelling, _)) = rest.split_once('"') else {
            continue;
        };
        found.push(spelling.to_string());
    }
    assert!(
        !found.is_empty(),
        "no arms found for {enum_name}::{function}"
    );
    found
}

/// Every kind the tool prints can be read back as the kind it printed.
#[test]
fn every_symbol_kind_survives_the_round_trip() {
    let all = spellings("src/model.rs", "SymbolKind", "as_str");
    assert!(all.len() >= 21, "expected every variant, got {}", all.len());
    for printed in all {
        let read: SymbolKind = serde_json::from_str(&format!("\"{printed}\""))
            .unwrap_or_else(|e| panic!("the tool writes `{printed}` and cannot read it: {e}"));
        assert_eq!(
            read.as_str(),
            printed,
            "`{printed}` read back as `{}`",
            read.as_str()
        );
    }
}

/// The same for the tiers, which `--json` prints beside every reference.
#[test]
fn every_confidence_survives_the_round_trip() {
    for printed in spellings("src/model.rs", "Confidence", "as_str") {
        let read: Confidence = serde_json::from_str(&format!("\"{printed}\""))
            .unwrap_or_else(|e| panic!("`{printed}`: {e}"));
        assert_eq!(read.as_str(), printed);
    }
}

/// And for the entry-point vocabulary, which a catalogue author writes by hand and the
/// loader has to accept.
#[test]
fn every_entry_kind_survives_the_round_trip() {
    for printed in spellings("src/analysis/entrypoints.rs", "EntryKind", "as_str") {
        let read: fun_refactor::analysis::entrypoints::EntryKind =
            serde_json::from_str(&format!("\"{printed}\""))
                .unwrap_or_else(|e| panic!("a catalogue may say `kind: {printed}`: {e}"));
        assert_eq!(read.as_str(), printed);
    }
}

/// `Language::name` is an identifier too. It names catalogue entries, `fr capabilities`
/// rows and every per-language table, and `from_name` is its parser. A language the tool
/// prints and cannot read would be the same defect one type over.
#[test]
fn every_language_name_parses_back() {
    for language in Language::ALL {
        let printed = language.name();
        assert_eq!(
            Language::from_name(printed),
            Some(*language),
            "`{printed}` is printed for {language:?} and does not parse back"
        );
    }
}

/// And its serde spelling is that same identifier.
///
/// This test used to ask only whether `from_name(name())` round-trips, which it did all
/// along. The gap was serde: commands that build their JSON by hand call `name()` and
/// print `"go"`, while `fr duplicates` serializes its result struct and printed `"Go"`,
/// a spelling no other command emits and `from_name` cannot read.
#[test]
fn every_language_survives_the_round_trip() {
    for language in Language::ALL {
        let printed = language.name();
        let read: Language = serde_json::from_str(&format!("\"{printed}\""))
            .unwrap_or_else(|e| panic!("the tool writes `{printed}` and cannot read it: {e}"));
        assert_eq!(read, *language);
        assert_eq!(
            serde_json::to_string(language).expect("serializes"),
            format!("\"{printed}\""),
            "serde and name() must agree for {language:?}"
        );
    }
}
