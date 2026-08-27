//! Copy-paste detection.

use fun_refactor::analysis::duplicates::{self, Options};
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

/// A function long enough to clear the threshold, parameterised so copies can differ.
fn go_function(name: &str, var: &str) -> String {
    format!(
        "package p\n\nfunc {name}(items []int, factor int) int {{\n\
         \t{var} := 0\n\
         \tfor _, item := range items {{\n\
         \t\tif item > 0 {{\n\
         \t\t\t{var} += item * factor\n\
         \t\t}} else {{\n\
         \t\t\t{var} -= item / factor\n\
         \t\t}}\n\
         \t}}\n\
         \tif {var} > 100 {{\n\
         \t\t{var} = 100\n\
         \t}}\n\
         \treturn {var}\n\
         }}\n"
    )
}

fn options(min_tokens: usize) -> Options {
    Options {
        min_tokens: Some(min_tokens),
        ..Options::default()
    }
}

#[test]
fn the_same_code_in_two_files_is_one_finding_with_two_instances() {
    let a = go_function("alpha", "total");
    let b = go_function("beta", "total");
    let (_tmp, index) = workspace(&[("a/x.go", &a), ("b/y.go", &b)]);

    let classes = duplicates::find(&index, &options(40)).unwrap();
    assert_eq!(classes.len(), 1, "one duplication, got {classes:?}");
    assert_eq!(classes[0].instances.len(), 2);
    assert_eq!(classes[0].language, Language::Go);
    assert_ne!(
        classes[0].instances[0].file, classes[0].instances[1].file,
        "the two instances are the two files"
    );
}

#[test]
fn a_copy_with_renamed_variables_is_still_a_copy() {
    // The whole reason to compare structure: `grep` cannot find this one.
    let a = go_function("alpha", "total");
    let b = go_function("beta", "accumulator");
    let (_tmp, index) = workspace(&[("a/x.go", &a), ("b/y.go", &b)]);

    let structural = duplicates::find(&index, &options(40)).unwrap();
    assert_eq!(structural.len(), 1, "renaming is not a difference in shape");

    let exact = duplicates::find(
        &index,
        &Options {
            min_tokens: Some(40),
            exact: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        exact.is_empty(),
        "--exact asks the stricter question and gets the stricter answer: {exact:?}"
    );
}

#[test]
fn an_identical_copy_is_found_in_exact_mode_too() {
    let a = go_function("alpha", "total");
    let (_tmp, index) = workspace(&[("a/x.go", &a), ("b/y.go", &a)]);

    let exact = duplicates::find(
        &index,
        &Options {
            min_tokens: Some(40),
            exact: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(exact.len(), 1, "got {exact:?}");
}

#[test]
fn small_shapes_every_language_repeats_are_not_reported() {
    // Two files with the same trivial function.
    let src = "package p\n\nfunc get(x int) int {\n\treturn x\n}\n";
    let (_tmp, index) = workspace(&[("a/x.go", src), ("b/y.go", src)]);

    let classes = duplicates::find(&index, &Options::default()).unwrap();
    assert!(classes.is_empty(), "below the threshold: {classes:?}");
}

#[test]
fn only_the_largest_duplicated_block_is_reported() {
    // A duplicated function duplicates its body, its loop and its every statement.
    let a = go_function("alpha", "total");
    let b = go_function("beta", "total");
    let (_tmp, index) = workspace(&[("a/x.go", &a), ("b/y.go", &b)]);

    let classes = duplicates::find(&index, &options(10)).unwrap();
    assert_eq!(
        classes.len(),
        1,
        "a low threshold must not multiply one duplication into many: {:?}",
        classes
            .iter()
            .map(|c| (c.tokens, c.instances.len()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn one_occurrence_is_not_a_duplicate() {
    let a = go_function("alpha", "total");
    let (_tmp, index) = workspace(&[("a/x.go", &a)]);
    let classes = duplicates::find(&index, &options(10)).unwrap();
    assert!(classes.is_empty(), "got {classes:?}");
}

#[test]
fn instances_of_one_class_never_overlap_each_other() {
    // Repeating a block inside a single file: the two instances are real, and each
    // must be its own bytes.
    let body = "\tif x > 0 {\n\t\ty := x * 2\n\t\tz := y + 1\n\t\tuse(y, z)\n\t}\n";
    let src = format!("package p\n\nfunc f(x int) {{\n{body}{body}}}\n\nfunc use(a, b int) {{}}\n");
    let (_tmp, index) = workspace(&[("a.go", &src)]);

    // Counted, because the loop is the whole test: a run that found no class, or one
    // instance per class, compares nothing and passes.
    let mut compared = 0;
    for class in duplicates::find(&index, &options(15)).unwrap() {
        for (i, one) in class.instances.iter().enumerate() {
            for other in &class.instances[i + 1..] {
                compared += 1;
                if one.file == other.file {
                    assert!(
                        one.span.end <= other.span.start || other.span.end <= one.span.start,
                        "instances overlap: {one:?} and {other:?}"
                    );
                }
            }
        }
    }
    assert!(
        compared > 0,
        "no two instances were compared, so this checked nothing"
    );
}

#[test]
fn a_language_filter_narrows_the_report() {
    let go = go_function("alpha", "total");
    let (_tmp, index) = workspace(&[("a/x.go", &go), ("b/y.go", &go)]);

    let none = duplicates::find(
        &index,
        &Options {
            min_tokens: Some(40),
            languages: vec![Language::Python],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(none.is_empty(), "no Python here: {none:?}");

    let some = duplicates::find(
        &index,
        &Options {
            min_tokens: Some(40),
            languages: vec![Language::Go],
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(some.len(), 1);
}

#[test]
fn a_file_that_does_not_parse_is_named_rather_than_silently_skipped() {
    // Its structure cannot be trusted, so the comparison skips it, and a report that quietly
    // leaves files out reads as "no duplication here".
    let good = go_function("alpha", "total");
    let (_tmp, index) = workspace(&[
        ("a/x.go", good.as_str()),
        ("b/y.go", good.as_str()),
        ("c/broken.go", "package p\n\nfunc oops( {\n"),
    ]);

    let skipped = duplicates::unparsed(&index, &Options::default());
    assert_eq!(skipped.len(), 1, "got {skipped:?}");
    assert!(skipped[0].ends_with("broken.go"));
}

#[test]
fn the_redundant_count_is_what_factoring_out_would_save() {
    let a = go_function("alpha", "total");
    let (_tmp, index) = workspace(&[("a/x.go", &a), ("b/y.go", &a), ("c/z.go", &a)]);

    let classes = duplicates::find(&index, &options(40)).unwrap();
    assert_eq!(classes[0].instances.len(), 3);
    assert_eq!(
        classes[0].redundant_tokens(),
        classes[0].tokens * 2,
        "one copy has to stay"
    );
}

/// A stylesheet rule repeated verbatim.
fn css_rule(class: &str) -> String {
    let declarations = [
        "display: flex",
        "flex-direction: column",
        "gap: 12px",
        "padding: 16px",
        "margin: 0",
        "border: 1px solid #ddd",
        "border-radius: 8px",
        "background: #fff",
        "color: #222",
        "font-size: 14px",
        "line-height: 1.5",
    ]
    .map(|d| format!("  {d};\n"))
    .join("");
    format!(".{class} {{\n{declarations}}}\n")
}

#[test]
fn markup_is_measured_against_a_markup_floor() {
    let doubled = format!("{}\n{}", css_rule("card"), css_rule("panel"));
    let (_tmp, index) = workspace(&[("a.css", &doubled)]);
    let unstated = Options::default();
    assert_eq!(
        unstated.min_tokens, None,
        "the default states no floor, so each language gets its own."
    );
    let found = duplicates::find(&index, &unstated).unwrap();
    assert_eq!(
        found.len(),
        1,
        "a rule written twice is duplication a stylesheet reader can act on."
    );

    // A Go pair of about the same token count stays under the code floor.
    let go = format!(
        "{}\n{}",
        go_function("alpha", "total"),
        go_function("beta", "total")
    );
    let (_tmp2, index2) = workspace(&[("a/x.go", &go)]);
    assert!(
        duplicates::find(&index2, &unstated).unwrap().is_empty(),
        "a short Go pair is below the code floor"
    );
    assert!(
        !duplicates::find(&index2, &options(40)).unwrap().is_empty(),
        "and a stated floor still reaches it"
    );
}

#[test]
fn a_stated_floor_beats_the_language_default() {
    let doubled = format!("{}\n{}", css_rule("card"), css_rule("panel"));
    let (_tmp, index) = workspace(&[("a.css", &doubled)]);
    let found = duplicates::find(&index, &options(200)).unwrap();
    assert!(
        found.is_empty(),
        "asking for 200 tokens means 200, in every language."
    );
}
