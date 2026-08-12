//! A refusal that says what to do instead is making a claim, and nothing checked it.
//!
//! Nearly every refusal ends with a route: `fr delete` removes a declaration nothing
//! uses, `fr provenance` answers this instead, rename the file to `.scss` if that is
//! what was meant. The sentence is the whole value of the refusal. It is what turns a
//! "no" into a next step, and it was the one part of the message no test drove.

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;

/// A language, the files that exercise it, and the symbol to ask about.
type Case<'a> = (Language, &'a [(&'a str, &'a str)], &'a str);

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, Index) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let scanned = scan(dir.path(), &ScanOptions::default()).expect("scan");
    let index = Index::build_from_scan(&scanned).expect("index");
    let root = dir.path().to_path_buf();
    (dir, root, index)
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

#[test]
fn flow_sends_the_caller_to_a_route_that_exists_and_answers() {
    // The refusal written when `fr flow` stopped answering for languages that do not
    // execute. It named `fr provenance`, which is not a command, and promised an answer
    // for three languages provenance does not cover.
    let cases: &[Case] = &[
        (
            Language::Yaml,
            &[("conf.yaml", "base: &base 8\nservice:\n  size: *base\n")],
            "service",
        ),
        (
            Language::Hcl,
            &[(
                "main.tf",
                "locals {\n  base = 8\n}\n\noutput \"size\" {\n  value = local.base\n}\n",
            )],
            "size",
        ),
        (
            Language::Css,
            &[(
                "a.css",
                ":root {\n  --base: 8px;\n}\n\n.card {\n  padding: var(--base);\n}\n",
            )],
            "--base",
        ),
        (
            Language::Scss,
            &[("a.scss", "$base: 8px;\n\n.card {\n  padding: $base;\n}\n")],
            "$base",
        ),
    ];

    for (language, files, name) in cases {
        let (_tmp, _root, index) = workspace(files);
        let id = symbol(&index, name);

        let refusal = fun_refactor::analysis::flow::forward(&index, id, 3)
            .expect_err("flow does not answer here")
            .to_string();
        assert!(
            !refusal.contains("fr provenance"),
            "{language} names a command that does not exist: {refusal}"
        );
        assert!(
            refusal.contains("`fr flow` traces its provenance"),
            "{language} names no route: {refusal}"
        );

        let traced = fun_refactor::analysis::provenance::provenance(&index, id, 3)
            .unwrap_or_else(|e| panic!("{language}: the route named refuses: {e}"));
        assert!(
            !traced.is_empty(),
            "{language}: the route named returned no hops"
        );
    }
}

#[test]
fn every_language_flow_turns_away_is_told_one_of_exactly_two_things() {
    // The eight languages `fr flow` refuses, asked as a set and not as the handful a
    // fixture happened to cover. Each is either sent to provenance. Provenance has an arm for
    // it, or told there is nothing of either kind to trace.
    use fun_refactor::analysis::{flow, provenance};

    let mut routed = Vec::new();
    let mut dead_end = Vec::new();
    for language in Language::ALL {
        if flow::supports_flow(*language) {
            continue;
        }
        match provenance::supports_provenance(*language) {
            true => routed.push(language.name()),
            false => dead_end.push(language.name()),
        }
    }
    assert_eq!(routed, ["css", "scss", "hcl", "yaml", "helm"]);
    assert_eq!(dead_end, ["html", "xml", "markdown"]);
}

#[test]
fn a_language_with_no_substitution_model_is_not_sent_anywhere() {
    // HTML, XML and Markdown do not execute *and* have nothing to substitute. The
    // refusal promised provenance would answer; provenance stops at the first hop
    // saying it has no model to follow, and the matrix claimed the cell all the same.
    for (language, files, name) in [
        (
            Language::Xml,
            &[(
                "doc.xml",
                "<?xml version=\"1.0\"?>\n<!DOCTYPE doc [\n<!ENTITY base \"8\">\n]>\n\
                 <doc>\n  <size>&base;</size>\n</doc>\n",
            )][..],
            "base",
        ),
        (
            Language::Markdown,
            &[("r.md", "[home]: https://example.com\n\nSee [home].\n")][..],
            "home",
        ),
    ] {
        let (_tmp, _root, index) = workspace(files);
        let id = symbol(&index, name);

        let refusal = fun_refactor::analysis::flow::forward(&index, id, 3)
            .expect_err("flow does not answer here")
            .to_string();
        assert!(
            refusal.contains("no substitution model to trace either"),
            "{language} was promised a route that does not answer: {refusal}"
        );

        assert!(
            !fun_refactor::capabilities::support(
                fun_refactor::capabilities::Capability::Provenance,
                language
            )
            .is_yes(),
            "{language} is claimed for provenance and the analysis has no arm for it"
        );
    }
}

#[test]
fn inline_sends_the_caller_to_delete_and_delete_removes_it() {
    // Six languages refuse to inline a binding nothing reads, each naming `fr delete`
    // as the thing that was probably meant. Whether `fr delete` would take it was never
    // asked in any of them.
    let cases: &[Case] = &[
        (
            Language::Rust,
            &[("a.rs", "pub fn f() {\n    let unread = 41;\n}\n")],
            "unread",
        ),
        (
            Language::Hcl,
            &[("main.tf", "locals {\n  unread = 8\n}\n")],
            "unread",
        ),
        (
            Language::Css,
            &[("a.css", ":root {\n  --unread: 8px;\n}\n")],
            "--unread",
        ),
        (
            Language::Markdown,
            &[("r.md", "# Title\n\n[unread]: https://example.com\n")],
            "unread",
        ),
        (
            Language::Bash,
            &[("run.sh", "#!/usr/bin/env bash\nUNREAD=\"x\"\necho hello\n")],
            "UNREAD",
        ),
    ];

    for (language, files, name) in cases {
        let (_tmp, _root, index) = workspace(files);
        let id = symbol(&index, name);

        let refusal = fun_refactor::refactor::inline::variable(&index, id)
            .expect_err("nothing reads it, so there is nothing to inline")
            .to_string();
        assert!(
            refusal.contains("fr delete"),
            "{language} refuses without naming a route: {refusal}"
        );

        // The claim: that route takes this symbol.
        let plan = fun_refactor::refactor::delete::plan(&index, id).unwrap_or_else(|e| {
            panic!("{language}: inline sent the caller to `fr delete` and delete refused: {e}")
        });
        assert!(
            !plan.edits.is_empty(),
            "{language}: `fr delete` planned nothing for the symbol it was handed"
        );
    }
}

#[test]
fn the_xml_entity_inline_refusal_names_a_command_that_takes_it() {
    // The sixth of the six `use `fr delete` if that is the intent` sites, reached
    // through its own entry point and not through `inline::variable`.
    let (_tmp, root, index) = workspace(&[(
        "doc.xml",
        "<?xml version=\"1.0\"?>\n<!DOCTYPE doc [\n<!ENTITY unread \"8\">\n]>\n<doc/>\n",
    )]);
    let file = root.join("doc.xml");

    let refusal = fun_refactor::refactor::inline::xml_entity(&file, "unread")
        .expect_err("nothing references it")
        .to_string();
    assert!(refusal.contains("Use `fr delete`"), "{refusal}");

    let plan = fun_refactor::refactor::delete::plan(&index, symbol(&index, "unread"))
        .expect("the refusal named `fr delete`, so `fr delete` has to take it");
    assert!(!plan.edits.is_empty(), "delete planned nothing");
}

#[test]
fn the_guard_clause_refusal_names_a_rewrite_that_applies() {
    // "this `if` has an `else`; invert it instead of guarding", so `InvertIf` has to
    // accept the very position `GuardClause` just turned away.
    let (_tmp, root, index) = workspace(&[(
        "a.rs",
        "pub fn pick(b: bool) -> u8 {\n    if b {\n        1\n    } else {\n        2\n    }\n}\n",
    )]);
    let file = root.join("a.rs");
    let source = std::fs::read_to_string(&file).expect("the file");
    let at = source.find("if b").expect("the if");

    let refusal = fun_refactor::refactor::rewrite::apply(
        &index,
        &file,
        at,
        fun_refactor::refactor::rewrite::Rewrite::GuardClause,
    )
    .expect_err("an `if` with an `else` is not a guard")
    .to_string();
    assert!(refusal.contains("invert it instead"), "{refusal}");

    let inverted = fun_refactor::refactor::rewrite::apply(
        &index,
        &file,
        at,
        fun_refactor::refactor::rewrite::Rewrite::InvertIf,
    )
    .expect("the refusal said to invert it, so inverting has to work here");
    assert!(!inverted.edits.is_empty(), "invert planned nothing");
}

#[test]
fn the_css_extract_refusal_names_a_rename_that_works() {
    // "rename {} to `.scss` if that is what was meant", the same bytes under the SCSS
    // grammar have to extract, or the advice sends the reader in a circle.
    let content = ".card {\n  color: red;\n  padding: 4px;\n}\n";
    let (_tmp, root, index) = workspace(&[("a.css", content)]);
    let file = root.join("a.css");
    let source = std::fs::read_to_string(&file).expect("the file");
    let start = source.find("  color").expect("the first declaration");
    let end = source.find("px;").expect("the last") + 3;
    let span = fun_refactor::span::Span::new(start, end);

    let refusal = fun_refactor::refactor::extract::function(&index, &file, span, "lifted")
        .expect_err("plain CSS has nothing to extract into")
        .to_string();
    assert!(refusal.contains("Rename"), "{refusal}");
    assert!(refusal.contains(".scss"), "{refusal}");

    // Same bytes, the name the refusal asked for.
    let (_tmp2, root2, index2) = workspace(&[("a.scss", content)]);
    let file2 = root2.join("a.scss");
    let plan = fun_refactor::refactor::extract::function(&index2, &file2, span, "lifted")
        .expect("the refusal said to rename it, so the renamed file has to extract");
    assert!(!plan.edits.is_empty(), "extract planned nothing on .scss");
}

#[test]
fn the_python_cycle_refusal_names_a_destination_that_works() {
    // "Move the names '{}' uses as well, or move it to a file neither imports". The second half
    // is the one a reader can act on without further thought. So it has to be true: a third
    // file, importing nothing and imported by nothing, must take it. The cycle: `holder` still
    // uses `width` after the move, so it would import it back, and `util` already imports from
    // `holder`.
    let files: &[(&str, &str)] = &[
        (
            "holder.py",
            "def width():\n    return 2\n\n\ndef area():\n    return width() * 3\n",
        ),
        (
            "util.py",
            "from holder import area\n\n\ndef helper():\n    return area()\n",
        ),
        ("spare.py", "\n"),
    ];
    let (_tmp, root, index) = workspace(files);
    let width = symbol(&index, "width");

    let refusal =
        fun_refactor::refactor::move_symbol::to_file(&index, width, &root.join("util.py"))
            .expect_err("that destination is a cycle")
            .to_string();
    assert!(
        refusal.contains("move it to a file neither imports"),
        "{refusal}"
    );

    // The destination the sentence describes.
    let plan = fun_refactor::refactor::move_symbol::to_file(&index, width, &root.join("spare.py"))
        .expect("the refusal named this destination, so it has to be accepted");
    assert!(!plan.edits.is_empty(), "the move planned nothing");
}

#[test]
fn the_rust_module_path_refusal_names_a_destination_that_works() {
    // "move it somewhere under `src/` instead".
    let (_tmp, root, index) = workspace(&[
        (
            "Cargo.toml",
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub fn width() -> usize {\n    1\n}\n"),
    ]);
    let width = symbol(&index, "width");

    let refusal =
        fun_refactor::refactor::move_symbol::to_file(&index, width, &root.join("outside.rs"))
            .expect_err("no module path can be derived out there")
            .to_string();
    assert!(
        refusal.contains("that the module tree already declares"),
        "the advice has to say what `src/` alone does not give you: {refusal}"
    );

    // Following the old advice exactly, a path under `src/`, landed on a second
    // refusal, because Rust reaches a file through a `mod` declaration and not through
    // its directory. That refusal now names the missing line.
    let second =
        fun_refactor::refactor::move_symbol::to_file(&index, width, &root.join("src/moved.rs"))
            .expect_err("nothing declares `mod moved;`")
            .to_string();
    assert!(
        second.contains("Add `mod moved;`"),
        "the second refusal names no route either: {second}"
    );

    // And the route it names works.
    let (_tmp2, root2, index2) = workspace(&[
        (
            "Cargo.toml",
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod moved;\n\npub fn width() -> usize {\n    1\n}\n",
        ),
        ("src/moved.rs", "\n"),
    ]);
    let plan = fun_refactor::refactor::move_symbol::to_file(
        &index2,
        symbol(&index2, "width"),
        &root2.join("src/moved.rs"),
    )
    .expect("with the module declared, the move the refusal described has to go through");
    assert!(!plan.edits.is_empty(), "the move planned nothing");
}

#[test]
fn the_ambiguous_flag_refusal_names_a_form_the_command_accepts() {
    // "say which one with a position", and `fr remove-flag` took a bare name and nothing else.
    // So the only route offered was one the command could not parse.
    use fun_refactor::refactor::cascade::{self, FlagTarget};

    let files: &[(&str, &str)] = &[
        (
            "a/one.rs",
            "pub const USE_NEW: bool = true;\n\npub fn go() -> u8 {\n    if USE_NEW { 1 } else { 2 }\n}\n",
        ),
        (
            "b/two.rs",
            "pub const USE_NEW: bool = true;\n\npub fn stop() -> u8 {\n    if USE_NEW { 3 } else { 4 }\n}\n",
        ),
    ];
    let (_tmp, root, _index) = workspace(files);

    let refusal = cascade::remove_flag(&root, "USE_NEW", true)
        .expect_err("two declarations, one name")
        .to_string();
    assert!(
        refusal.contains("`path:line:col`"),
        "the refusal has to name the form the command takes: {refusal}"
    );
    assert!(
        refusal.contains("one.rs:1") && refusal.contains("two.rs:1"),
        "and say where they are: {refusal}"
    );

    // The form it names, for each declaration in turn.
    for (file, other) in [("a/one.rs", "b/two.rs"), ("b/two.rs", "a/one.rs")] {
        let path = root.join(file);
        let source = std::fs::read_to_string(&path).expect("the file");
        let offset = source.find("USE_NEW").expect("the declaration");
        let plan = cascade::remove_flag_for(&root, &FlagTarget::At(path, offset), true)
            .unwrap_or_else(|e| panic!("the refusal named this form and it was refused: {e}"));

        // Resolved to a name, not carried around as a position: everything downstream looks the
        // flag up by name. A plan that says `one.rs:10` found nothing.
        assert!(
            plan.rounds
                .iter()
                .any(|r| r.description.contains("USE_NEW")),
            "the plan describes the flag by position: {:?}",
            plan.rounds
                .iter()
                .map(|r| &r.description)
                .collect::<Vec<_>>()
        );
        assert!(
            plan.edits.paths().any(|p| p.ends_with(file)),
            "{file} was the target and was not edited"
        );
        assert!(
            !plan.edits.paths().any(|p| p.ends_with(other)),
            "{other} was not the target and was edited anyway"
        );
    }
}

#[test]
fn the_go_cycle_refusal_names_a_package_that_works() {
    // "Move what '{}' uses as well, or move it to a package neither imports".
    let files: &[(&str, &str)] = &[
        ("go.mod", "module example.com/m\n\ngo 1.21\n"),
        (
            "holder/holder.go",
            "package holder\n\nfunc Width() int {\n\treturn 2\n}\n\nfunc Area() int {\n\treturn Width() * 3\n}\n",
        ),
        (
            "util/util.go",
            "package util\n\nimport \"example.com/m/holder\"\n\nfunc Helper() int {\n\treturn holder.Area()\n}\n",
        ),
        ("spare/spare.go", "package spare\n"),
    ];
    let (_tmp, root, index) = workspace(files);
    let width = symbol(&index, "Width");

    let refusal =
        fun_refactor::refactor::move_symbol::to_file(&index, width, &root.join("util/moved.go"))
            .expect_err("that destination is an import cycle")
            .to_string();
    assert!(
        refusal.contains("move it to a package neither imports"),
        "{refusal}"
    );

    let plan =
        fun_refactor::refactor::move_symbol::to_file(&index, width, &root.join("spare/moved.go"))
            .expect("the refusal named this destination, so it has to be accepted");
    assert!(!plan.edits.is_empty(), "the move planned nothing");
}

#[test]
fn the_unread_flag_refusal_names_a_command_that_takes_it() {
    // "'{flag}' is declared at {} and nothing reads it, so there is no flag to remove.
    // `fr delete` removes a declaration nothing uses."
    let (_tmp, root, index) = workspace(&[("a.rs", "pub const USE_NEW: bool = true;\n")]);

    let refusal = fun_refactor::refactor::cascade::remove_flag(&root, "USE_NEW", true)
        .expect_err("nothing reads it")
        .to_string();
    assert!(
        refusal.contains("`fr delete` removes a declaration"),
        "{refusal}"
    );

    let plan = fun_refactor::refactor::delete::plan(&index, symbol(&index, "USE_NEW"))
        .expect("the refusal named `fr delete`, so `fr delete` has to take it");
    assert!(!plan.edits.is_empty(), "delete planned nothing");
}

#[test]
fn the_no_cascade_here_reason_names_two_commands_that_work() {
    // The matrix's reason for every language without conditionals: "removing one is a
    // rename or a delete instead of a cascade". Two commands named to a reader who
    // cannot have the third, so both have to be there for that language.
    use fun_refactor::capabilities::{support, Capability};

    let cases: &[Case] = &[
        (
            Language::Yaml,
            &[("conf.yaml", "flags:\n  use_new: true\n")],
            "use_new",
        ),
        (
            Language::Xml,
            &[(
                "doc.xml",
                "<?xml version=\"1.0\"?>\n<!DOCTYPE doc [\n<!ENTITY use_new \"true\">\n]>\n<doc/>\n",
            )],
            "use_new",
        ),
        (
            Language::Markdown,
            &[("r.md", "# Title\n\n[use_new]: https://example.com\n")],
            "use_new",
        ),
    ];

    for (language, files, name) in cases {
        let reason = support(Capability::RemoveFlag, *language)
            .reason()
            .unwrap_or_else(|| panic!("{language} is claimed for remove flag"));
        assert!(
            reason.contains("a rename or a delete"),
            "{language}: {reason}"
        );

        assert!(
            support(Capability::Rename, *language).is_yes(),
            "{language} is sent to `fr rename`, which the matrix does not claim for it"
        );
        assert!(
            support(Capability::SafeDelete, *language).is_yes(),
            "{language} is sent to `fr delete`, which the matrix does not claim for it"
        );

        let (_tmp, _root, index) = workspace(files);
        let id = symbol(&index, name);
        fun_refactor::refactor::rename::plan(&index, id, "use_old")
            .unwrap_or_else(|e| panic!("{language}: sent to `fr rename` and it refused: {e}"));
        fun_refactor::refactor::delete::plan(&index, id)
            .unwrap_or_else(|e| panic!("{language}: sent to `fr delete` and it refused: {e}"));
    }
}

#[test]
fn the_bash_digit_refusal_names_a_spelling_that_works() {
    // "`$12` is not parameter 12 … Write it as `${12}` first if that is what was meant." A
    // shell reads `$1` then the literal `2`. So a function with twelve parameters cannot be
    // rewritten until the author says which they meant. The advice is only worth printing if
    // the braced spelling then goes through.
    let many = (1..=12)
        .map(|n| format!("  local a{n}=\"${{{n}}}\"\n"))
        .collect::<String>();
    let ambiguous = format!(
        "#!/usr/bin/env bash\n\nwide() {{\n{many}  echo \"$12\"\n}}\n\nwide a b c d e f g h i j k l\n"
    );
    let (_tmp, _root, index) = workspace(&[("run.sh", &ambiguous)]);
    let wide = symbol(&index, "wide");

    let refusal = fun_refactor::refactor::signature::change(
        &index,
        wide,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .expect_err("`$12` is `$1` followed by a `2`")
    .to_string();
    assert!(refusal.contains("Write it as `${12}` first"), "{refusal}");

    // The spelling the refusal asked for.
    let braced = ambiguous.replace("\"$12\"", "\"${12}\"");
    let (_tmp2, _root2, index2) = workspace(&[("run.sh", &braced)]);
    let plan = fun_refactor::refactor::signature::change(
        &index2,
        symbol(&index2, "wide"),
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .expect("the refusal named this spelling, so it has to be accepted");
    assert!(!plan.edits.is_empty(), "the change planned nothing");
}

#[test]
fn the_helm_list_refusal_names_an_input_that_is_accepted() {
    // "`{a,b}` … names no single key to rank; pass the list in a `-f` values file
    // instead." So a `-f` file holding that same list has to be taken and ranked.
    let refusal = fun_refactor::helm::parse_set("ports={80,443}", false)
        .expect_err("a list literal names no single key")
        .to_string();
    assert!(refusal.contains("`-f` values file instead"), "{refusal}");

    let (_tmp, root, index) = workspace(&[
        ("Chart.yaml", "name: demo\nversion: 0.1.0\n"),
        ("values.yaml", "ports:\n  - 8080\n"),
        (
            "templates/cm.yaml",
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: c\ndata:\n  ports: {{ .Values.ports }}\n",
        ),
        ("override.yaml", "ports:\n  - 80\n  - 443\n"),
    ]);

    let inputs = fun_refactor::analysis::provenance::ValuesInputs {
        files: vec![root.join("override.yaml")],
        sets: Vec::new(),
    };
    let ports = symbol(&index, "ports");
    let traced =
        fun_refactor::analysis::provenance::provenance_with_inputs(&index, ports, 5, &inputs)
            .expect("the refusal named a `-f` file, so a `-f` file has to be taken");
    assert!(
        !traced.is_empty(),
        "the input the refusal named was accepted and then traced nothing"
    );
    assert!(
        traced
            .hops
            .iter()
            .any(|h| h.file.ends_with("override.yaml")),
        "the `-f` file was not ranked into the answer: {:?}",
        traced.hops.iter().map(|h| &h.file).collect::<Vec<_>>()
    );
}

#[test]
fn the_missing_chart_refusal_names_the_file_that_unblocks_it() {
    // "no Chart.yaml above {}, so the chart name is unknown", the route is the file it names,
    // so adding one has to be enough.
    //
    // Reached through a `.tpl`, which is Helm by its extension alone. Every other Helm file is
    // Helm *because* a Chart.yaml sits above it. So this refusal cannot arise for one: the
    // fixture that looked obvious tested a plain YAML file instead.
    let template = "{{- define \"demo.labels\" -}}\napp: demo\ntier: web\n{{- end -}}\n";
    let (_tmp, root, index) = workspace(&[("templates/_helpers.tpl", template)]);
    let file = root.join("templates/_helpers.tpl");
    let source = std::fs::read_to_string(&file).expect("the file");
    let span = fun_refactor::span::Span::new(
        source.find("app: demo").expect("the first line"),
        source.find("tier: web").expect("the second") + "tier: web".len(),
    );

    let refusal = fun_refactor::refactor::extract::function(&index, &file, span, "block")
        .expect_err("no chart name can be derived")
        .to_string();
    assert!(refusal.contains("no Chart.yaml above"), "{refusal}");

    // The same tree, with the file the refusal named.
    let (_tmp2, root2, index2) = workspace(&[
        ("Chart.yaml", "name: demo\nversion: 0.1.0\n"),
        ("templates/_helpers.tpl", template),
    ]);
    let plan = fun_refactor::refactor::extract::function(
        &index2,
        &root2.join("templates/_helpers.tpl"),
        span,
        "block",
    )
    .expect("the refusal named Chart.yaml, so adding one has to unblock it");
    assert!(!plan.edits.is_empty(), "extract planned nothing");
}
