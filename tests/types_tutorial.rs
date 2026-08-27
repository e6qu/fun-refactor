//! The stages of the types tutorial, measured and not asserted.

use fun_refactor::analysis::types;
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolKind;
use fun_refactor::scan::ScanOptions;
use std::path::{Path, PathBuf};

/// The stages, in the order the page walks them.
const STAGES: &[&str] = &[
    "stage0_as_found",
    "stage1_annotated",
    "stage2_named_ids",
    "stage3_closed_sets",
    "stage4_grouped",
    "stage5_unconstructible",
    "stage6_state_machine",
    "stage7_deleted",
];

fn tutorial() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/types_tutorial")
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Known {
    declared: usize,
    inferred: usize,
    unknown: usize,
}

impl Known {
    fn total(&self) -> usize {
        self.declared + self.inferred + self.unknown
    }
}

/// What the tool knows about every value in one stage, in one language.
fn known_in(stage: &str, language: Language) -> Known {
    let index = Index::build(&tutorial().join(stage), &ScanOptions::default()).expect("an index");
    let mut known = Known::default();
    for symbol in &index.symbols {
        if symbol.language != language {
            continue;
        }
        // Values, not the types and functions that describe them.
        if !matches!(
            symbol.kind,
            SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Parameter | SymbolKind::Field
        ) {
            continue;
        }
        let answer = types::of(&index, symbol.id).expect("a type answer");
        match (&answer.declared, &answer.inferred) {
            (Some(_), _) => known.declared += 1,
            (None, Some(_)) => known.inferred += 1,
            (None, None) => known.unknown += 1,
        }
    }
    known
}

#[test]
fn every_stage_parses_in_both_languages() {
    for stage in STAGES {
        let index = Index::build(&tutorial().join(stage), &ScanOptions::default())
            .unwrap_or_else(|e| panic!("{stage}: {e}"));
        for language in [Language::Python, Language::TypeScript] {
            assert!(
                index.symbols.iter().any(|s| s.language == language),
                "{stage} has no {language} at all"
            );
        }
    }
}

#[test]
fn the_first_stage_has_written_down_nothing() {
    // The whole point of where it starts.
    for language in [Language::Python, Language::TypeScript] {
        let known = known_in(STAGES[0], language);
        assert_eq!(
            known.declared, 0,
            "{language} at stage 0 declares something: {known:?}"
        );
        assert!(known.total() > 0, "{language} at stage 0 has no values");
    }
}

#[test]
fn what_the_source_says_never_goes_backwards() {
    // Each stage declares at least as much as the one before it.
    for language in [Language::Python, Language::TypeScript] {
        let mut previous = 0;
        for stage in STAGES {
            let known = known_in(stage, language);
            assert!(
                known.declared >= previous,
                "{language} at {stage} declares {} where the stage before declared {previous}",
                known.declared
            );
            previous = known.declared;
        }
    }
}

#[test]
fn the_last_stage_leaves_almost_nothing_unknown() {
    // Not "nothing": the page is honest that a handful of values stay beyond it, and
    // this is the number that keeps that honest in both directions.
    for language in [Language::Python, Language::TypeScript] {
        let known = known_in(STAGES[STAGES.len() - 1], language);
        let settled = known.declared + known.inferred;
        assert!(
            settled * 10 >= known.total() * 9,
            "{language} at the last stage settles {settled} of {} values: {known:?}",
            known.total()
        );
    }
}

#[test]
fn the_states_carry_only_what_that_state_has() {
    // The climax of the page, checked and not described: a failure has a reason and no capture
    // time.
    let index = Index::build(
        &tutorial().join("stage6_state_machine"),
        &ScanOptions::default(),
    )
    .expect("an index");
    let fields_of = |owner: &str, language: Language| -> Vec<String> {
        index
            .symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Field
                    && s.language == language
                    && s.qualifier.as_deref() == Some(owner)
            })
            .map(|s| s.name.clone())
            .collect()
    };
    for language in [Language::Python, Language::TypeScript] {
        let captured = fields_of("Captured", language);
        let failed = fields_of("Failed", language);
        assert!(
            captured
                .iter()
                .any(|f| f.to_lowercase().contains("captured")),
            "{language}: Captured has no capture time: {captured:?}"
        );
        assert!(
            !failed.iter().any(|f| f.to_lowercase().contains("captured")),
            "{language}: Failed carries a capture time: {failed:?}"
        );
        assert!(
            failed.iter().any(|f| f == "reason"),
            "{language}: Failed has no reason: {failed:?}"
        );
        assert!(
            !captured.iter().any(|f| f == "reason"),
            "{language}: Captured carries a reason: {captured:?}"
        );
    }
}

#[test]
fn the_last_stage_deleted_the_checks_the_one_before_still_had() {
    // The payoff.
    for name in ["payments.py", "payments.ts"] {
        let before = std::fs::read_to_string(tutorial().join("stage6_state_machine").join(name))
            .expect("stage 6");
        let after =
            std::fs::read_to_string(tutorial().join("stage7_deleted").join(name)).expect("stage 7");
        for gone in [
            "not authorized",
            "vendor cannot receive payouts",
            "not captured",
        ] {
            assert!(before.contains(gone), "{name}: stage 6 lacks `{gone}`");
            assert!(
                !after.contains(gone),
                "{name}: stage 7 still checks for `{gone}`"
            );
        }
        // And the one that stays, because it relates two values instead of a state.
        assert!(
            after.contains("exceeds"),
            "{name}: stage 7 dropped the amount check too"
        );
    }
}
