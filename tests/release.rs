//! The release workflow and what the README promises it produces.

use std::collections::BTreeSet;

const WORKFLOW: &str = include_str!("../.github/workflows/release.yml");
const CI: &str = include_str!("../.github/workflows/ci.yml");
const README: &str = include_str!("../README.md");
const MANIFEST: &str = include_str!("../.release-please-manifest.json");
const CARGO: &str = include_str!("../Cargo.toml");
const CONFIG: &str = include_str!("../release-please-config.json");

/// Every `target:` the binaries matrix names.
fn targets() -> BTreeSet<String> {
    WORKFLOW
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- target: "))
        .map(|t| t.trim().to_string())
        .collect()
}

#[test]
fn every_target_built_is_a_target_the_readme_offers() {
    let missing: Vec<String> = targets()
        .into_iter()
        .filter(|target| !README.contains(&format!("`fr-<tag>-{target}.tar.gz`")))
        .collect();
    assert!(
        missing.is_empty(),
        "the release builds {missing:?} and the README offers no download for \
         them. An artifact nobody looks for is one nobody takes."
    );
}

#[test]
fn every_download_the_readme_offers_is_built() {
    // Read out of the table rather than listed here, so a row added to the
    // README without a job behind it fails instead of going unnoticed.
    let built = targets();
    let mut promised = Vec::new();
    for line in README.lines() {
        let Some(at) = line.find("`fr-<tag>-") else {
            continue;
        };
        let rest = &line[at + "`fr-<tag>-".len()..];
        let Some(end) = rest.find(".tar.gz`") else {
            continue;
        };
        promised.push(rest[..end].to_string());
    }
    assert!(
        promised.len() >= 4,
        "the README's download table lists {} archive(s), which is too few to \
         cover two systems and two architectures: {promised:?}",
        promised.len()
    );
    let unbuilt: Vec<&String> = promised.iter().filter(|t| !built.contains(*t)).collect();
    assert!(
        unbuilt.is_empty(),
        "the README offers {unbuilt:?} and no job builds them. That download is \
         a 404 and the workflow that made the release was green."
    );
}

#[test]
fn both_systems_and_both_architectures_are_covered() {
    // The promise in as many words: Linux and macOS, amd64 and arm64.
    let built = targets();
    for wanted in [
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        assert!(
            built.contains(wanted),
            "no job builds {wanted}. The release covers {built:?}."
        );
    }
    // And the browser build, which is not a target in the matrix because it is
    // packaged differently: a module and its loader rather than an executable.
    assert!(
        WORKFLOW.contains("wasm32-unknown-unknown"),
        "the release workflow builds nothing for the browser."
    );
    assert!(
        README.contains("`fun-refactor-<tag>-wasm.tar.gz`"),
        "the README's download table offers no browser build."
    );
}

#[test]
fn every_artifact_travels_with_its_checksum() {
    // A download nobody can verify is a download nobody should run.
    let packaged = WORKFLOW.matches("shasum -a 256").count();
    let uploaded = WORKFLOW.matches(".tar.gz.sha256\"").count();
    assert!(
        packaged >= 2 && uploaded >= 2,
        "{packaged} package step(s) write a checksum and {uploaded} upload step(s) \
         send one. Every artifact needs both."
    );
}

#[test]
fn the_two_versions_agree() {
    // `release-please` writes the version into both.
    let manifest = MANIFEST
        .split('"')
        .nth(3)
        .expect("the manifest holds one version");
    let cargo = CARGO
        .lines()
        .find_map(|l| l.strip_prefix("version = "))
        .map(|v| v.trim().trim_matches('"'))
        .expect("Cargo.toml holds a version");
    assert_eq!(
        manifest, cargo,
        "`.release-please-manifest.json` says {manifest} and `Cargo.toml` says \
         {cargo}. The next release would repeat a version."
    );
}

#[test]
fn the_release_reads_the_config_this_repository_holds() {
    // A workflow pointing at a config file that is not there falls back to
    // defaults and releases something else, quietly.
    assert!(
        WORKFLOW.contains("config-file: release-please-config.json"),
        "the release job does not name this repository's config file."
    );
    assert!(
        WORKFLOW.contains("manifest-file: .release-please-manifest.json"),
        "the release job does not name this repository's manifest."
    );
    assert!(
        CONFIG.contains("\"release-type\": \"rust\""),
        "the config does not say this is a Rust package, so `Cargo.toml` and \
         `Cargo.lock` would not move with the version."
    );
}

#[test]
fn the_kinds_of_change_the_title_gate_takes_are_the_kinds_the_changelog_sorts() {
    // The gate on a pull request title decides what may be merged.
    let gate = CI
        .lines()
        .find(|l| l.contains("pattern='^("))
        .expect("CI holds the title pattern");
    let kinds: Vec<&str> = gate
        .split_once("'^(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inner, _)| inner.split('|').collect())
        .expect("the pattern lists the kinds");
    assert!(kinds.len() >= 8, "the gate lists only {kinds:?}");
    for kind in &kinds {
        // `revert` is a kind `release-please` handles itself rather than one it
        // is given a section for.
        if *kind == "revert" {
            continue;
        }
        assert!(
            CONFIG.contains(&format!("\"type\": \"{kind}\"")),
            "a pull request titled `{kind}: …` may be merged, and the changelog \
             config has no section for it."
        );
    }
}

/// Every workspace member's manifest, by the member's path.
fn members() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let listed = CARGO
        .split_once("members = [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inner, _)| inner.to_string())
        .expect("the workspace lists its members");
    listed
        .split(',')
        .map(|m| m.trim().trim_matches('"').to_string())
        .filter(|m| !m.is_empty())
        .map(|m| {
            let path = match m.as_str() {
                "." => root.join("Cargo.toml"),
                other => root.join(other).join("Cargo.toml"),
            };
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()));
            (m, text)
        })
        .collect()
}

#[test]
fn every_member_carries_the_same_version() {
    // `release-please` gives every member of a Rust workspace the root's version.
    let root = CARGO
        .lines()
        .find_map(|l| l.strip_prefix("version = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("the root declares a version");
    let adrift: Vec<String> = members()
        .into_iter()
        .filter_map(|(name, text)| {
            let version = text
                .lines()
                .find_map(|l| l.strip_prefix("version = "))?
                .trim()
                .trim_matches('"')
                .to_string();
            (version != root).then_some(format!("{name} is {version}"))
        })
        .collect();
    assert!(
        adrift.is_empty(),
        "the root is {root} and {adrift:?}. Every member moves together."
    );
}

#[test]
fn a_platform_dependency_on_a_member_carries_a_version() {
    // `release-please` rewrites the version of every dependency naming a member.
    // Under `[target.'cfg(...)'.dependencies]` it writes the key without checking
    // for one, and throws where none is there.
    let names: Vec<String> = members()
        .iter()
        .filter_map(|(_, text)| {
            text.lines()
                .find_map(|l| l.strip_prefix("name = "))
                .map(|n| n.trim().trim_matches('"').to_string())
        })
        .collect();
    assert!(names.len() > 3, "only {names:?} were read as members");

    let mut bare = Vec::new();
    let mut in_target = false;
    for line in CARGO.lines() {
        if line.starts_with('[') {
            in_target = line.starts_with("[target.");
            continue;
        }
        if !in_target {
            continue;
        }
        let Some((name, rest)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !names.iter().any(|n| n == name) {
            continue;
        }
        if rest.contains("path") && !rest.contains("version") {
            bare.push(name.to_string());
        }
    }
    assert!(
        bare.is_empty(),
        "{bare:?} name a workspace member from a platform table with no `version`. \
         The release job throws on that. Add `version` beside `path`."
    );
}

#[test]
fn the_browser_check_reads_where_the_release_writes() {
    // The release job wrote the bindings where nothing loads them from.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let check = std::fs::read_to_string(root.join("web/test/api.mjs"))
        .expect("the browser check is readable");
    let loaded = check
        .lines()
        .find_map(|l| {
            l.split_once("join(root, \"")
                .and_then(|(_, r)| r.split_once('/'))
        })
        .map(|(dir, _)| format!("web/{dir}/wasm"))
        .expect("the check names the directory it loads from");
    assert!(
        WORKFLOW.contains(&format!("--out-dir {loaded}")),
        "the release generates bindings somewhere other than {loaded}, which is \
         where the browser check loads them from."
    );
    assert!(
        WORKFLOW.contains(&format!("cp {loaded}/*")),
        "the release packages bindings from somewhere other than {loaded}."
    );
    assert!(
        CI.contains(&format!("--out-dir {loaded}")),
        "CI generates bindings somewhere other than {loaded}, so the release and \
         the gate build different things."
    );
}

#[test]
fn an_archive_is_named_from_the_version_and_not_the_tag() {
    // Naming from the tag doubled the package name; renaming the tag lost release-please its own last release.
    assert!(
        !WORKFLOW.contains("name=\"fr-${{ needs.release-please.outputs.tag }}"),
        "an archive takes its name from the tag, which carries the package name."
    );
    for shape in [
        "name=\"fr-v${{ needs.release-please.outputs.version }}",
        "name=\"fun-refactor-v${{ needs.release-please.outputs.version }}",
    ] {
        assert!(
            WORKFLOW.contains(shape),
            "no packaging step names its archive `{shape}…`."
        );
    }
    assert!(
        !CONFIG.contains("include-component-in-tag"),
        "the tag format moved. release-please finds its last release by tag, and \
         a format it has never written sends it past one."
    );
}

#[test]
fn no_job_asks_for_a_runner_that_is_retired() {
    // A job naming a retired image waits for a runner that never comes.
    for retired in ["macos-13", "macos-11", "macos-12", "ubuntu-20.04"] {
        assert!(
            !WORKFLOW.contains(retired) && !CI.contains(retired),
            "a job asks for {retired}, which GitHub no longer offers. It waits \
             for a runner that never comes."
        );
    }
}
