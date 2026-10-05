use super::*;

fn fixture() -> (PathBuf, Sources) {
    let root = std::env::temp_dir().join("fr-recipe-index-tests");
    let sources = BTreeMap::from([(
        root.join("module.py"),
        (
            Language::Python,
            "def original():\n    return 1\n\ndef caller():\n    return original()\n".to_string(),
        ),
    )]);
    (root, sources)
}

#[test]
fn empty_selectors_build_at_most_one_index_per_recipe() {
    let (root, sources) = fixture();
    let file = super::super::parse(include_str!(
        "../../tests/fixtures/recipe-index/unchanged.recipe"
    ))
    .unwrap();
    let outer = crate::vfs::new_handle(std::iter::empty());
    crate::vfs::with_handle(&outer, || {
        INDEX_BUILDS.with(|count| count.set(0));
        let (report, after) = run_file(
            &file,
            sources.clone(),
            &Options {
                root: &root,
                catalogs: &[],
            },
        )
        .unwrap();
        assert!(report.ok, "{report:?}");
        assert_eq!(after, sources);
        let builds = INDEX_BUILDS.with(|count| count.get());
        assert!(
            (1..=2).contains(&builds),
            "unchanged recipes built {builds} indexes"
        );
        assert_eq!(
            crate::vfs::read_to_string(root.join("module.py")).unwrap(),
            sources[&root.join("module.py")].1
        );
    });
}

#[test]
fn zero_limit_keeps_match_counts_without_speculative_indexing() {
    let (root, sources) = fixture();
    let file = super::super::parse(include_str!(
        "../../tests/fixtures/recipe-index/zero-limit.recipe"
    ))
    .unwrap();
    let outer = crate::vfs::new_handle(std::iter::empty());
    crate::vfs::with_handle(&outer, || {
        INDEX_BUILDS.with(|count| count.set(0));
        let (report, after) = run(
            &file.recipes[0],
            sources.clone(),
            &Options {
                root: &root,
                catalogs: &[],
            },
        )
        .unwrap();
        assert!(report.ok, "{report:?}");
        assert_eq!(after, sources);
        assert_eq!(INDEX_BUILDS.with(|count| count.get()), 1);
    });
}

#[test]
fn real_edits_between_empty_steps_refresh_symbols_references_and_changed_files() {
    let (root, sources) = fixture();
    let file = super::super::parse(include_str!(
        "../../tests/fixtures/recipe-index/edits.recipe"
    ))
    .unwrap();
    let outer = crate::vfs::new_handle(std::iter::empty());
    crate::vfs::with_handle(&outer, || {
        let (report, after) = run(
            &file.recipes[0],
            sources,
            &Options {
                root: &root,
                catalogs: &[],
            },
        )
        .unwrap();
        assert!(report.ok, "{report:?}");
        let text = &after[&root.join("module.py")].1;
        assert!(
            text.contains("def finished()") && text.contains("return finished()"),
            "{text}"
        );
        assert!(
            !text.contains("original") && !text.contains("intermediate"),
            "{text}"
        );
        assert_eq!(
            crate::vfs::read_to_string(root.join("module.py")).unwrap(),
            *text
        );
    });
}

#[test]
fn allowed_refusal_and_empty_step_preserve_the_next_edit() {
    let (root, sources) = fixture();
    let file = super::super::parse(include_str!(
        "../../tests/fixtures/recipe-index/refusal.recipe"
    ))
    .unwrap();
    let outer = crate::vfs::new_handle(std::iter::empty());
    crate::vfs::with_handle(&outer, || {
        let (report, after) = run(
            &file.recipes[0],
            sources,
            &Options {
                root: &root,
                catalogs: &[],
            },
        )
        .unwrap();
        assert!(report.ok, "{report:?}");
        assert!(after[&root.join("module.py")]
            .1
            .contains("return finished()"));
    });
}
