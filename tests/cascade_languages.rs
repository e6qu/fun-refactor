//! Cascading flag removal in Zig, shell and Terraform.
//!
//! The three languages fail differently, and the tests are arranged around that. Zig writes its
//! `if` twice, as a statement and as an expression, so both spellings need collapsing. Shell
//! has no booleans at all: substituting the flag leaves a *string* inside a test, and only some
//! of the ways a script can test a string are decidable. So what is refused matters as much as
//! what is collapsed. Terraform has no `if`: the flag reaches a resource through `count`, and
//! removing it as false deletes the resource, which strands every address pointing at it.
//!
//! Every language gets the true case, the false case, a multi-round cascade, a symbol that
//! survives because something else still uses it, a reparse of the result, and each refusal by
//! name.

use fun_refactor::edit::{apply_to_string, Validation};
use fun_refactor::refactor::cascade;
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    tmp
}

fn result_for(tmp: &Path, file: &str, plan: &cascade::CascadePlan) -> String {
    let path = tmp.join(file);
    let original = std::fs::read_to_string(&path).unwrap();
    match plan.edits.edits_for(&path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

/// Every cascade must leave every file it touched parseable.
fn must_still_parse(plan: &cascade::CascadePlan) {
    fun_refactor::edit::plan(&plan.edits, Validation::ReparseStrict)
        .expect("a cascade must not leave a file broken");
}

fn unfinished(plan: &cascade::CascadePlan) -> String {
    plan.unfinished.join("\n")
}

// =========================================================================== zig

#[test]
fn zig_true_keeps_the_enabled_branch_and_removes_the_dead_one() {
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         fn onlyOld() void {}\n\
         \n\
         pub fn run() void {\n\
         \x20   if (USE_NEW) {\n\
         \x20       newPath();\n\
         \x20   } else {\n\
         \x20       onlyOld();\n\
         \x20   }\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.zig", &plan);

    assert_eq!(
        out,
        "pub fn run() void {\n\
         \x20   newPath();\n\
         }\n"
    );
    assert!(
        plan.rounds.len() >= 3,
        "expected a cascade: {:?}",
        plan.rounds
    );
    must_still_parse(&plan);
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

#[test]
fn zig_false_keeps_the_else_branch() {
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run() void {\n\
         \x20   if (USE_NEW) {\n\
         \x20       newPath();\n\
         \x20   } else {\n\
         \x20       oldPath();\n\
         \x20   }\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    assert_eq!(
        result_for(tmp.path(), "a.zig", &plan),
        "pub fn run() void {\n\
         \x20   oldPath();\n\
         }\n"
    );
    must_still_parse(&plan);
}

#[test]
fn zig_collapses_an_if_used_as_an_expression() {
    // `if (cond) a else b` is a value in Zig. It is not just a statement. It has no
    // `body`/`alternative` fields at all, the branches are positional.
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn limit() u8 {\n\
         \x20   const n = if (USE_NEW) 10 else 1;\n\
         \x20   return n;\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "a.zig", &plan),
        "pub fn limit() u8 {\n\
         \x20   const n = 10;\n\
         \x20   return n;\n\
         }\n"
    );
    must_still_parse(&plan);
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

#[test]
fn zig_keeps_the_semicolon_an_unbraced_branch_never_had() {
    // `if (c) f() else g();` puts the semicolon on the `if`, not on the branch, so a
    // naive collapse produces `f()` and a parse error.
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run() void {\n\
         \x20   if (USE_NEW) newPath() else oldPath();\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "a.zig", &plan),
        "pub fn run() void {\n\
         \x20   newPath();\n\
         }\n"
    );
    must_still_parse(&plan);
}

#[test]
fn zig_keeps_nesting_inside_the_surviving_branch() {
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run(x: bool) void {\n\
         \x20   if (USE_NEW) {\n\
         \x20       if (x) {\n\
         \x20           deep();\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "a.zig", &plan),
        "pub fn run(x: bool) void {\n\
         \x20   if (x) {\n\
         \x20       deep();\n\
         \x20   }\n\
         }\n"
    );
    must_still_parse(&plan);
}

#[test]
fn zig_an_else_if_becomes_the_if_without_gaining_a_semicolon() {
    // The kept branch is a whole `if` statement. It is not an expression, so the semicolon
    // rule for unbraced branches must not fire on it.
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run(x: bool) void {\n\
         \x20   if (USE_NEW) {\n\
         \x20       newPath();\n\
         \x20   } else if (x) {\n\
         \x20       oldPath();\n\
         \x20   }\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    assert_eq!(
        result_for(tmp.path(), "a.zig", &plan),
        "pub fn run(x: bool) void {\n\
         \x20   if (x) {\n\
         \x20       oldPath();\n\
         \x20   }\n\
         }\n"
    );
    must_still_parse(&plan);
}

#[test]
fn zig_a_function_still_used_elsewhere_survives() {
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         fn shared() void {}\n\
         \n\
         pub fn other() void {\n\
         \x20   shared();\n\
         }\n\
         \n\
         pub fn run() void {\n\
         \x20   if (USE_NEW) {\n\
         \x20       newPath();\n\
         \x20   } else {\n\
         \x20       shared();\n\
         \x20   }\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.zig", &plan);
    assert!(out.contains("fn shared() void {}"), "got:\n{out}");
    assert!(out.contains("newPath();"), "got:\n{out}");
    must_still_parse(&plan);
}

#[test]
fn zig_a_payload_capture_is_refused_by_name() {
    // A payload binds the condition's value, and a boolean has no value to bind, so
    // the substitution stays and the `if` is reported instead of guessed at.
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run() void {\n\
         \x20   if (USE_NEW) |v| {\n\
         \x20       use(v);\n\
         \x20   }\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.zig", &plan);
    assert!(out.contains("if (true) |v|"), "substitution stays:\n{out}");
    assert_eq!(plan.unfinished.len(), 1, "{}", unfinished(&plan));
    // The line is the one in the finished file. It is not the one it started on. The flag's own
    // declaration went, so everything below it moved up.
    assert!(
        plan.unfinished[0].contains("payload") && plan.unfinished[0].contains("a.zig:2"),
        "{}",
        unfinished(&plan)
    );
}

#[test]
fn zig_an_expression_conditional_without_an_else_is_refused_by_name() {
    // Zig has no such expression, so this file does not parse; the point is that the
    // cascade says so instead of emitting a half-collapsed file.
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run() void {\n\
         \x20   const n = if (USE_NEW) 1;\n\
         \x20   use(n);\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert!(
        unfinished(&plan).contains("no `else`"),
        "{}",
        unfinished(&plan)
    );
}

#[test]
fn zig_a_labelled_block_branch_is_kept_whole() {
    // `blk: { … }` is a statement in its own right; stripping its braces would strand
    // the `break :blk` inside it.
    let tmp = workspace(&[(
        "a.zig",
        "const USE_NEW = true;\n\
         \n\
         pub fn run() u8 {\n\
         \x20   const n = if (USE_NEW) blk: {\n\
         \x20       break :blk 1;\n\
         \x20   } else 2;\n\
         \x20   return n;\n\
         }\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.zig", &plan);
    assert!(out.contains("break :blk 1;"), "got:\n{out}");
    assert!(out.contains("blk:"), "the label must survive:\n{out}");
    must_still_parse(&plan);
}

// ========================================================================== bash

#[test]
fn bash_true_collapses_a_quoted_string_test() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         only_old() {\n\
         \x20 echo old\n\
         }\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 new_path\n\
         else\n\
         \x20 only_old\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(result_for(tmp.path(), "run.sh", &plan), "new_path\n");
    assert!(
        plan.rounds.len() >= 3,
        "expected a cascade: {:?}",
        plan.rounds
    );
    must_still_parse(&plan);
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

#[test]
fn bash_false_keeps_the_else_branch() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 new_path\n\
         else\n\
         \x20 old_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    assert_eq!(result_for(tmp.path(), "run.sh", &plan), "old_path\n");
    must_still_parse(&plan);
}

#[test]
fn bash_collapses_the_bare_command_form() {
    // `if $FLAG; then` runs the variable's value as a command, so substituting gives
    // `if true; then`, the shell builtin, which is decidable.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if $USE_NEW; then\n\
         \x20 new_path\n\
         else\n\
         \x20 old_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    assert_eq!(result_for(tmp.path(), "run.sh", &plan), "old_path\n");
    must_still_parse(&plan);
}

#[test]
fn bash_collapses_double_bracket_and_test_spellings() {
    for condition in [
        "[[ \"$USE_NEW\" == true ]]",
        "test \"$USE_NEW\" = true",
        "[ \"$USE_NEW\" != false ]",
    ] {
        let tmp = workspace(&[(
            "run.sh",
            &format!("USE_NEW=true\n\nif {condition}; then\n  new_path\nelse\n  old_path\nfi\n"),
        )]);
        let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
        assert_eq!(
            result_for(tmp.path(), "run.sh", &plan),
            "new_path\n",
            "condition: {condition}"
        );
        must_still_parse(&plan);
    }
}

#[test]
fn bash_a_function_still_used_elsewhere_survives() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         shared() {\n\
         \x20 echo shared\n\
         }\n\
         \n\
         other() {\n\
         \x20 shared\n\
         }\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 new_path\n\
         else\n\
         \x20 shared\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "run.sh", &plan);
    assert!(out.contains("shared() {"), "it still has a caller:\n{out}");
    must_still_parse(&plan);
}

#[test]
fn bash_keeps_indentation_inside_the_surviving_branch() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 for f in *; do\n\
         \x20   echo \"$f\"\n\
         \x20 done\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "run.sh", &plan),
        "for f in *; do\n\x20 echo \"$f\"\ndone\n"
    );
    must_still_parse(&plan);
}

#[test]
fn bash_lifts_a_nested_conditional_out_intact() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 if other; then\n\
         \x20   deep\n\
         \x20 fi\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "run.sh", &plan),
        "if other; then\n\x20 deep\nfi\n"
    );
    must_still_parse(&plan);
}

#[test]
fn bash_substitutes_inside_a_larger_string_without_stranding_the_sigil() {
    // Replacing only the name would leave `"pre$true"`, so the expansion goes with it. A string
    // that was nothing *but* the expansion loses its quotes too: they existed to protect a
    // value that might have had spaces in it. `true` is a bare word, leaving them on would hide
    // the literal from every shell test.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\necho \"pre$USE_NEW\" \"${USE_NEW}\"\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "run.sh", &plan),
        "echo \"pretrue\" true\n"
    );
    must_still_parse(&plan);
}

#[test]
fn bash_a_compound_expansion_is_refused_by_name() {
    // `${FLAG:-no}` means more than the flag's value: the whole expansion cannot be replaced
    // without losing the default. The name alone cannot be replaced without producing
    // `${true:-no}`.
    //
    // The assignment is the only other place the name appears, so refusing that one use leaves
    // nothing to do. Removing the assignment on its own would have turned a script that read
    // `true` into a script that reads `no`.
    let tmp = workspace(&[("run.sh", "USE_NEW=true\n\necho \"${USE_NEW:-no}\"\n")]);

    let error = cascade::remove_flag(tmp.path(), "USE_NEW", true)
        .expect_err("a flag whose every use is refused cannot be removed")
        .to_string();
    assert!(error.contains("not a plain expansion"), "{error}");
    assert!(error.contains("nothing was changed"), "{error}");
}

#[test]
fn bash_a_refused_use_keeps_the_assignment_that_feeds_it() {
    // One use can be replaced and one cannot. The flag still has a reader, so the
    // assignment has to stay: taking it away would leave `${USE_NEW:-no}` reading `no`.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 go\n\
         fi\n\
         echo \"${USE_NEW:-no}\"\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "run.sh", &plan);
    assert!(out.contains("USE_NEW=true"), "got:\n{out}");
    assert!(out.contains("${USE_NEW:-no}"), "got:\n{out}");
    assert!(
        unfinished(&plan).contains("not a plain expansion"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn bash_a_test_against_another_variable_is_refused_by_name() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = \"$OTHER\" ]; then\n\
         \x20 new_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "run.sh", &plan);
    assert!(out.contains("if [ true = \"$OTHER\" ]"), "got:\n{out}");
    assert!(
        unfinished(&plan).contains("not a provably constant shell test"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn bash_a_one_operand_test_is_refused_because_false_is_a_non_empty_string() {
    // `[ false ]` succeeds: a one-operand test asks whether its operand is non-empty.
    // Collapsing it would be faithful to the shell and almost certainly wrong about
    // what the author meant, so it is handed back.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" ]; then\n\
         \x20 new_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    let out = result_for(tmp.path(), "run.sh", &plan);
    assert!(out.contains("if [ false ]"), "got:\n{out}");
    assert!(
        unfinished(&plan).contains("non-empty string"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn bash_a_false_if_with_an_elif_is_refused_by_name() {
    // Keeping the `elif` means promoting it to an `if`, which is a different rewrite.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 new_path\n\
         elif other; then\n\
         \x20 middle\n\
         else\n\
         \x20 old_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    let out = result_for(tmp.path(), "run.sh", &plan);
    assert!(out.contains("elif other; then"), "got:\n{out}");
    assert!(
        unfinished(&plan).contains("promoting an `elif`"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn bash_a_true_if_with_an_elif_drops_the_whole_chain() {
    // The other direction needs no promotion: a true test wins outright.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 new_path\n\
         elif other; then\n\
         \x20 middle\n\
         else\n\
         \x20 old_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert_eq!(result_for(tmp.path(), "run.sh", &plan), "new_path\n");
    must_still_parse(&plan);
}

#[test]
fn bash_an_unrelated_conditional_is_not_touched() {
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\
         \n\
         if [ -f /etc/hosts ]; then\n\
         \x20 echo yes\n\
         fi\n\
         \n\
         if [ \"$USE_NEW\" = true ]; then\n\
         \x20 new_path\n\
         fi\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "run.sh", &plan);
    assert!(out.contains("if [ -f /etc/hosts ]; then"), "got:\n{out}");
    assert!(out.contains("new_path"), "got:\n{out}");
    assert!(!out.contains("USE_NEW"), "got:\n{out}");
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

// =========================================================================== hcl

#[test]
fn terraform_true_drops_a_count_of_one_and_the_variable() {
    // `count = 1` is what a resource does by default. So the argument goes with the flag
    // instead of being left as a line of noise.
    let tmp = workspace(&[
        (
            "variables.tf",
            "variable \"enabled\" {\n  type    = bool\n  default = true\n}\n",
        ),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs\"\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"logs\"\n}\n"
    );
    assert_eq!(result_for(tmp.path(), "variables.tf", &plan), "");
    must_still_parse(&plan);
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

#[test]
fn terraform_false_deletes_the_resource_the_count_zeroed() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs\"\n\
             }\n\
             \n\
             resource \"aws_s3_bucket\" \"keep\" {\n\
             \x20 bucket = \"keep\"\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", false).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"keep\" {\n  bucket = \"keep\"\n}\n"
    );
    must_still_parse(&plan);
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

#[test]
fn terraform_reports_the_addresses_a_deleted_resource_leaves_dangling() {
    // Deleting whatever referenced the resource would turn a flag removal into an
    // open-ended configuration change, so the dangling addresses are handed back.
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs\"\n\
             }\n\
             \n\
             output \"arn\" {\n\
             \x20 value = aws_s3_bucket.logs[0].arn\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", false).unwrap();
    let out = result_for(tmp.path(), "main.tf", &plan);
    assert!(
        !out.contains("resource \"aws_s3_bucket\" \"logs\""),
        "got:\n{out}"
    );
    assert!(
        out.contains("value = aws_s3_bucket.logs[0].arn"),
        "the dangling use is reported, not deleted:\n{out}"
    );
    assert_eq!(plan.unfinished.len(), 1, "{}", unfinished(&plan));
    assert!(
        plan.unfinished[0].contains("aws_s3_bucket.logs no longer exists")
            && plan.unfinished[0].contains("main.tf:2"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn terraform_collapses_a_conditional_in_any_attribute() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 bucket = var.enabled ? \"new\" : \"old\"\n\
             \x20 acl    = var.enabled ? \"private\" : \"public\"\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", false).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"logs\" {\n\
         \x20 bucket = \"old\"\n\
         \x20 acl    = \"public\"\n\
         }\n"
    );
    must_still_parse(&plan);
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
}

#[test]
fn terraform_deletes_a_resource_a_false_for_each_empties() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 for_each = var.enabled ? var.buckets : {}\n\
             \x20 bucket   = each.value\n\
             }\n\
             \n\
             resource \"aws_s3_bucket\" \"keep\" {\n\
             \x20 bucket = \"keep\"\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", false).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"keep\" {\n  bucket = \"keep\"\n}\n"
    );
    must_still_parse(&plan);
}

#[test]
fn terraform_a_true_for_each_keeps_the_collection_it_chose() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 for_each = var.enabled ? var.buckets : {}\n\
             \x20 bucket   = each.value\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"logs\" {\n\
         \x20 for_each = var.buckets\n\
         \x20 bucket   = each.value\n\
         }\n"
    );
    must_still_parse(&plan);
}

#[test]
fn terraform_a_count_the_author_wrote_by_hand_survives() {
    // Only a `count` this cascade produced may be removed; one that was already there
    // belongs to the author.
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = 1\n\
             \x20 bucket = var.enabled ? \"new\" : \"old\"\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"logs\" {\n\
         \x20 count  = 1\n\
         \x20 bucket = \"new\"\n\
         }\n"
    );
    must_still_parse(&plan);
}

#[test]
fn terraform_a_resource_used_elsewhere_survives_a_true_removal() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs\"\n\
             }\n\
             \n\
             resource \"aws_sqs_queue\" \"q\" {\n\
             \x20 name = \"q\"\n\
             }\n\
             \n\
             output \"queue\" {\n\
             \x20 value = aws_sqs_queue.q.arn\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    let out = result_for(tmp.path(), "main.tf", &plan);
    assert!(
        out.contains("resource \"aws_sqs_queue\" \"q\""),
        "an unrelated resource is untouched:\n{out}"
    );
    assert!(out.contains("value = aws_sqs_queue.q.arn"), "got:\n{out}");
    assert!(!out.contains("count"), "the count of 1 goes:\n{out}");
    assert!(plan.unfinished.is_empty(), "{}", unfinished(&plan));
    must_still_parse(&plan);
}

#[test]
fn terraform_keeps_a_count_of_one_the_module_indexes_into() {
    // `count` is not only a number: it makes `aws_s3_bucket.logs` a list.
    // Deleting it would leave `[0]` indexing a single object, which will not plan.
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs\"\n\
             }\n",
        ),
        (
            "outputs.tf",
            "output \"arn\" {\n  value = aws_s3_bucket.logs[0].arn\n}\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    assert_eq!(
        result_for(tmp.path(), "main.tf", &plan),
        "resource \"aws_s3_bucket\" \"logs\" {\n\
         \x20 count  = 1\n\
         \x20 bucket = \"logs\"\n\
         }\n"
    );
    assert!(
        unfinished(&plan).contains("read with an index"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn terraform_keeps_a_count_of_one_that_count_index_depends_on() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs-${count.index}\"\n\
             }\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    let out = result_for(tmp.path(), "main.tf", &plan);
    assert!(out.contains("count  = 1"), "got:\n{out}");
    assert!(
        unfinished(&plan).contains("`count.index` is used here"),
        "{}",
        unfinished(&plan)
    );
    must_still_parse(&plan);
}

#[test]
fn terraform_a_zeroed_resource_goes_even_when_it_was_indexed() {
    // The false case is not the same problem. The resource is gone either way, so the index has
    // nothing to point at and is reported as dangling.
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n\
             \x20 count  = var.enabled ? 1 : 0\n\
             \x20 bucket = \"logs\"\n\
             }\n",
        ),
        (
            "outputs.tf",
            "output \"arn\" {\n  value = aws_s3_bucket.logs[0].arn\n}\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", false).unwrap();
    assert_eq!(result_for(tmp.path(), "main.tf", &plan), "");
    assert!(
        unfinished(&plan).contains("aws_s3_bucket.logs no longer exists"),
        "{}",
        unfinished(&plan)
    );
}

#[test]
fn terraform_reading_through_the_flag_is_refused_by_name() {
    // `var.enabled.field` reads into the value; a boolean has no field to read, so
    // the substitution cannot stand in for the whole traversal.
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = any\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n  bucket = var.enabled.name\n}\n",
        ),
    ]);

    // That one traversal is the variable's only reader. So refusing it leaves nothing to do,
    // and deleting the declaration under it would leave `var.enabled.name` pointing at a
    // variable Terraform no longer declares.
    let error = cascade::remove_flag(tmp.path(), "enabled", true)
        .expect_err("a variable whose every use is refused cannot be removed")
        .to_string();
    assert!(error.contains("reads through the flag"), "{error}");
    assert!(error.contains("nothing was changed"), "{error}");
}

#[test]
fn terraform_a_refused_traversal_keeps_the_variable_it_reads() {
    // One use reads the value and one reads through it. The second keeps the variable
    // alive, so the declaration stays even though the first was replaced.
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = any\n}\n"),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n  \
             count  = var.enabled ? 1 : 0\n  \
             bucket = var.enabled.name\n}\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", true).unwrap();
    assert!(
        unfinished(&plan).contains("reads through the flag"),
        "{}",
        unfinished(&plan)
    );
    let out = result_for(tmp.path(), "main.tf", &plan);
    assert!(out.contains("var.enabled.name"), "got:\n{out}");
    assert_eq!(
        result_for(tmp.path(), "variables.tf", &plan),
        "variable \"enabled\" {\n  type = any\n}\n",
        "the declaration still has a reader, so it stays"
    );
}

#[test]
fn terraform_resolves_the_variable_across_the_module_directory() {
    let tmp = workspace(&[
        ("variables.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        (
            "buckets.tf",
            "resource \"aws_s3_bucket\" \"a\" {\n  count = var.enabled ? 1 : 0\n}\n",
        ),
        (
            "queues.tf",
            "resource \"aws_sqs_queue\" \"b\" {\n  count = var.enabled ? 1 : 0\n}\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "enabled", false).unwrap();
    assert_eq!(result_for(tmp.path(), "buckets.tf", &plan), "");
    assert_eq!(result_for(tmp.path(), "queues.tf", &plan), "");
    assert_eq!(result_for(tmp.path(), "variables.tf", &plan), "");
}

// ================================================================ across languages

#[test]
fn a_language_without_a_collapse_step_refuses_rather_than_going_quiet() {
    // This test used to assert that a substituted-but-uncollapsed YAML file is reported rather
    // than left silent, over a fixture where `use_new. USE_NEW` was supposed to read the Python
    // flag. It never does: YAML's only reference edge is the anchor and alias, which the query
    // says in as many words. So `remove_flag` bailed, the whole body sat behind `if let
    // Ok(plan)`, and the test asserted nothing from the day it was written.
    //
    // Driving it properly found the real defect. The matrix answers `supports_cascade` for this
    // cell and the command never asked. So `n/a` was a claim with nothing behind it: an XML
    // entity flag *was* substituted, `&use_new;` became `&true;`, an entity no document
    // defines. The prolog went with the declaration. The command asks the same predicate now,
    // so the contract below is a refusal.
    //
    // The substituted-but-uncollapsed report has no reachable input any more. This says why
    // and not pretending to cover it: the definition's language is checked above. A
    // reference in one of these languages never resolves better than `NameOnly`, which is not
    // safe to rewrite. All seven were tried.
    let tmp = workspace(&[(
        "doc.xml",
        "<?xml version=\"1.0\"?>\n<!DOCTYPE doc [\n<!ENTITY use_new \"true\">\n]>\n\
         <doc>\n  <flag>&use_new;</flag>\n</doc>\n",
    )]);

    let said = cascade::remove_flag(tmp.path(), "use_new", true)
        .expect_err("xml has no conditional for a flag to guard")
        .to_string();

    assert!(
        said.contains("is not supported for xml"),
        "it refuses by name: {said}"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("doc.xml")).expect("the file"),
        "<?xml version=\"1.0\"?>\n<!DOCTYPE doc [\n<!ENTITY use_new \"true\">\n]>\n\
         <doc>\n  <flag>&use_new;</flag>\n</doc>\n",
        "and leaves the document as it was"
    );
}

#[test]
fn an_unknown_flag_is_an_error_in_every_language() {
    for (name, source) in [
        ("a.zig", "pub fn run() void {}\n"),
        ("run.sh", "echo hello\n"),
        ("main.tf", "resource \"a\" \"b\" {}\n"),
    ] {
        let tmp = workspace(&[(name, source)]);
        let err = cascade::remove_flag(tmp.path(), "NOT_THERE", true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no symbol named"), "{name}: {err}");
    }
}

#[test]
fn rounds_are_reported_for_every_language() {
    let cases: [(&str, &str, &str); 3] = [
        (
            "a.zig",
            "USE_NEW",
            "const USE_NEW = true;\n\npub fn run() void {\n    if (USE_NEW) {\n        go();\n    }\n}\n",
        ),
        (
            "run.sh",
            "USE_NEW",
            "USE_NEW=true\n\nif [ \"$USE_NEW\" = true ]; then\n  go\nfi\n",
        ),
        (
            "main.tf",
            "enabled",
            "variable \"enabled\" {\n  type = bool\n}\n\nresource \"a\" \"b\" {\n  x = var.enabled ? 1 : 2\n}\n",
        ),
    ];

    for (name, flag, source) in cases {
        let tmp = workspace(&[(name, source)]);
        let plan = cascade::remove_flag(tmp.path(), flag, true).unwrap();
        assert!(plan.rounds.len() >= 2, "{name}: {:?}", plan.rounds);
        assert!(plan.rounds[0].description.contains(flag), "{name}");
        assert!(plan.rounds.iter().all(|r| r.files_touched > 0), "{name}");
    }
}
