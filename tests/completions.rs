//! `fr completions <shell>` writes a script the shell can read.

mod common;

use common::require_on_ci;
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn script(shell: &str) -> String {
    let out = Command::new(FR)
        .args(["completions", shell])
        .output()
        .expect("fr should run");
    assert!(out.status.success(), "fr completions {shell} failed");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn help_commands() -> Vec<String> {
    let out = Command::new(FR).arg("--help").output().expect("fr --help");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .filter(|w| *w != "help")
        .map(str::to_string)
        .collect()
}

#[test]
fn every_shell_gets_a_script_its_own_parser_accepts() {
    let mut missing = Vec::new();
    let mut checked = 0;
    for shell in ["bash", "zsh"] {
        let path = std::env::temp_dir().join(format!("fr-completions-{shell}"));
        std::fs::write(&path, script(shell)).expect("write");
        // A shell that is not installed cannot judge its own script.
        let Ok(out) = Command::new(shell).arg("-n").arg(&path).output() else {
            missing.push(shell.to_string());
            continue;
        };
        assert!(
            out.status.success(),
            "{shell} refused its own completion script: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        checked += 1;
    }
    assert!(checked > 0, "no shell was available to check anything");
    // On CI a missing shell is a check that ran and proved nothing.
    require_on_ci("completion scripts", &missing);
}

#[test]
fn the_script_offers_every_command_the_help_lists() {
    let bash = script("bash");
    let fish = script("fish");
    let commands = help_commands();
    assert!(commands.len() > 20, "expected a command list: {commands:?}");
    for name in &commands {
        assert!(
            bash.contains(name.as_str()),
            "bash completion never mentions `{name}`"
        );
        assert!(
            fish.contains(name.as_str()),
            "fish completion never mentions `{name}`"
        );
    }
}

#[test]
fn a_command_offers_its_own_flags() {
    let bash = script("bash");
    assert!(
        bash.contains("--min-tokens"),
        "`duplicates` takes --min-tokens, and the script should say so"
    );
    assert!(
        bash.contains("--no-ignore"),
        "a global flag turns up wherever it applies"
    );
}
