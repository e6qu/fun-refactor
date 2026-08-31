//! Bash crosses as its computational subset.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const TALLY_SH: &str = "#!/usr/bin/env bash\n\n# Square a number.\nsquare() {\n    \
                        local n=\"$1\"\n    echo $(( n * n ))\n}\n\ntotal=0\n\
                        for i in 1 2 3; do\n    sq=$(square \"$i\")\n    \
                        total=$(( total + sq ))\ndone\n\
                        if [ \"$total\" -gt 10 ]; then\n    echo \"big: $total\"\n\
                        else\n    echo \"small: $total\"\nfi\n";

#[test]
fn a_bash_function_reads_its_stdout_as_its_return() {
    let (_tmp, root) = common::tree(&[("tally.sh", TALLY_SH)]);
    let plan = transpile::plan(&root.join("tally.sh"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains("def square(a1) -> int:"),
        "the parameter and the settled return type:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("return n * n"),
        "the final echo is the value the caller captures:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("sq = square(i)"),
        "the substitution reads as the call it is:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(transpile::MARKER),
        "nothing here is outside the subset:\n{}",
        plan.output
    );
}

#[test]
fn a_python_function_writes_its_return_onto_stdout() {
    let source = "def shout(word, times):\n    line = \"\"\n    i = 0\n    \
                  while i < times:\n        line = line + word\n        i = i + 1\n    \
                  return line\n\n\nprint(shout(\"ha\", 3))\n";
    let (_tmp, root) = common::tree(&[("shout.py", source)]);
    let plan = transpile::plan(&root.join("shout.py"), Language::Bash).expect("a draft");
    assert!(
        plan.output.contains("local word=\"$1\""),
        "parameters arrive positionally and keep their names:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("line=\"${line}${word}\""),
        "text joins as text, never as arithmetic:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("printf '%s\\n' \"${line}\"") && plan.output.contains("return 0"),
        "the returned value prints, and the status says it went well:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("\"$(shout \"ha\" 3)\""),
        "a caller captures the value from stdout:\n{}",
        plan.output
    );
}

#[test]
fn a_case_and_a_counted_loop_cross_in_both_directions() {
    let sh = "kind() {\n    local label=\"$1\"\n    case \"$label\" in\n        one)\n            \
              echo 1\n            ;;\n        two|three)\n            echo 23\n            ;;\n        \
              *)\n            echo 0\n            ;;\n    esac\n}\n\n\
              walk() {\n    for (( i=0; i < 3; i++ )); do\n        echo \"$i\"\n    done\n}\n";
    let (_tmp, root) = common::tree(&[("kind.sh", sh)]);
    let plan = transpile::plan(&root.join("kind.sh"), Language::Go).expect("a draft");
    assert!(
        plan.output.contains("switch label {") && plan.output.contains("case \"two\", \"three\":"),
        "the case selects the way the target selects:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("for i := 0; i < 3; i++ {"),
        "the counted loop keeps its header:\n{}",
        plan.output
    );
}

#[test]
fn what_bash_cannot_say_carries_loudly() {
    // A pipeline into an external program has no counterpart the targets share.
    let sh = "count() {\n    ls | wc -l\n}\n";
    let (_tmp, root) = common::tree(&[("count.sh", sh)]);
    let plan = transpile::plan(&root.join("count.sh"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER) && plan.output.contains("ls | wc -l"),
        "the pipeline is in the output, named as untranslated:\n{}",
        plan.output
    );
    assert!(plan.fidelity.carried_verbatim > 0);

    // And toward bash, a record has nowhere to go and says so.
    let py =
        "from dataclasses import dataclass\n\n\n@dataclass\nclass Point:\n    x: int\n    y: int\n";
    let (_tmp2, root2) = common::tree(&[("p.py", py)]);
    let plan = transpile::plan(&root2.join("p.py"), Language::Bash).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "the record carries loudly:\n{}",
        plan.output
    );
}

#[test]
fn a_bash_round_trip_keeps_the_program() {
    // bash -> python -> bash: the program's meaning survives two crossings.
    let (_tmp, root) = common::tree(&[("tally.sh", TALLY_SH)]);
    let plan = transpile::plan(&root.join("tally.sh"), Language::Python).expect("a draft");
    let (_tmp2, root2) = common::tree(&[("tally.py", &plan.output)]);
    let back = transpile::plan(&root2.join("tally.py"), Language::Bash).expect("a draft");
    assert!(
        back.output.contains("square() {") && back.output.contains("printf '%s\\n' $(( n * n ))"),
        "the function and its value survive the round trip:\n{}",
        back.output
    );
}
