//! Public reference mutation cannot retain a stale consumer lookup.

use fun_refactor::index::{Index, References};
use fun_refactor::lang::Language;
use fun_refactor::model::{Confidence, Reference, SymbolId};

fn fixture() -> Index {
    Index::build_from_sources(&[
        ("app.py".into(), Language::Python,
         "def café(value):\n    return value\ndef other(value):\n    return value\ndef run():\n    return café(1) + other(2) + café(3)\n".into()),
        ("overloads.ts".into(), Language::TypeScript,
         "function pick(x: string): string;\nfunction pick(x: number): number;\nfunction pick(x: any): any { return x; }\nconst result = pick(1);\n".into()),
    ]).unwrap()
}

fn verify(index: &Index) {
    for id in index
        .symbols
        .iter()
        .map(|s| s.id)
        .chain([SymbolId(u32::MAX)])
    {
        let group = index.definition_group(id);
        let expected = index
            .references
            .iter()
            .filter(|r| r.target.is_some_and(|t| group.contains(&t)))
            .collect::<Vec<_>>();
        let actual = index.references_to(id);
        assert_eq!(actual.len(), expected.len());
        assert_eq!(index.reference_count(id), expected.len());
        assert_eq!(index.has_references(id), !expected.is_empty());
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(std::ptr::eq(actual, expected));
        }
        let expected = index
            .references
            .iter()
            .filter(|r| index.symbol(id).is_some_and(|s| r.name == s.name) && r.target != Some(id))
            .collect::<Vec<_>>();
        let actual = index.unresolved_matching(id);
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(std::ptr::eq(actual, expected));
        }
    }
}

#[test]
fn counts_and_presence_keep_exact_function_consumers() {
    let index = fixture();
    for (name, count) in [("café", 2), ("other", 1), ("run", 0)] {
        let id = index.symbols.iter().find(|s| s.name == name).unwrap().id;
        assert_eq!(index.reference_count(id), count);
        assert_eq!(index.has_references(id), count > 0);
    }
    verify(&index);
}

#[test]
fn mutations_through_vector_borrows_rebuild_both_lookups() {
    let mut index = fixture();
    let original = index.references.clone().into_vec();
    let ids = index.symbols.iter().map(|s| s.id).collect::<Vec<_>>();
    let mut seed = 431u64;
    for iteration in 0..160 {
        verify(&index);
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        if index.references.is_empty() {
            index.references = original.clone().into();
        }
        let position = seed as usize % index.references.len();
        let references: &mut Vec<Reference> = &mut index.references;
        match iteration % 10 {
            0 => references[position].target = Some(ids[seed as usize % ids.len()]),
            1 => references[position].target = Some(SymbolId(u32::MAX)),
            2 => references[position].name = "café".into(),
            3 => references[position].confidence = Confidence::NameOnly,
            4 => references.push(references[position].clone()),
            5 => references.swap(0, position),
            6 => {
                references.drain(..position);
            }
            7 => references.retain(|r| r.target.is_some()),
            8 => references.extend(original.iter().take(3).cloned()),
            _ => references.truncate(position),
        }
    }
    verify(&index);
}

#[test]
fn replacement_clone_and_serialization_keep_only_current_records() {
    let mut index = fixture();
    verify(&index);
    let json = serde_json::to_value(&index.references).unwrap();
    assert_eq!(json, serde_json::to_value(&*index.references).unwrap());
    let restored: References = serde_json::from_value(json).unwrap();
    index.references = restored;
    verify(&index);
    let detached = index.references.clone();
    index.references.clear();
    verify(&index);
    index.references = detached.into_iter().collect();
    verify(&index);
    let middle = index.references.len() / 2;
    let replacement = index.references.split_off(middle);
    verify(&index);
    index.references = replacement.into();
    verify(&index);
    let extracted = std::mem::take(&mut index.references);
    verify(&index);
    index.references = extracted.into_vec().into();
    verify(&index);
}

#[test]
fn simultaneous_first_reads_agree_then_mutation_refreshes() {
    let mut index = fixture();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| verify(&index));
        }
    });
    for reference in &mut index.references {
        reference.target = None;
    }
    verify(&index);
}

#[test]
fn symbol_group_changes_do_not_reuse_cached_groups() {
    let mut index = fixture();
    verify(&index);
    let picks = index
        .symbols
        .iter()
        .filter(|s| s.name == "pick")
        .map(|s| s.id)
        .collect::<Vec<_>>();
    assert!(picks.len() > 1);
    index.symbols[picks[0].0 as usize].container = Some(picks[1]);
    verify(&index);
}
