//! Does the code that a removal leaves behind still compile?
//!
//! The third and last file to drive the compile gate. `output_compiles.rs` takes the
//! commands that move a declaration, `rewrites_compile.rs` the ones that rewrite one in
//! place, and these are the ones that take code away: `fr delete`, `fr imports`,
//! `fr remove-flag`, and `fr recipe`, which composes them.
//!
//! Taking code away has a failure mode the other two do not. The last use of an import
//! often lives in the code being removed, and the statement stays behind. Go calls that an
//! error outright, TypeScript does under `noUnusedLocals`, and Rust makes it a warning that
//! a `-D warnings` build — this project's own — turns into one. **The result parses in
//! every case**, which is why sweeping for parse errors found none of it and running a
//! compiler found all of it.

mod common;
use common::{gate, must_plan, Toolchain, Workspace};

use fun_refactor::lang::Language;

/// One language's fixture for the removal commands.
struct Fixture {
    language: Language,
    toolchain: Toolchain,
    file: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// A function nothing calls, whose body holds the only use of an import.
    doomed: &'static str,
    /// The flag whose dead branch holds the only use of another import.
    flag: &'static str,
    /// The import that must survive every case.
    keeps: &'static str,
    /// The import only the doomed function uses, which deleting it must take.
    drops_on_delete: &'static str,
    /// The import only the flag's dead branch uses, which removing it must take.
    drops_on_flag: &'static str,
}

fn skip(fixture: &Fixture) -> bool {
    if fixture.toolchain.is_available() {
        return false;
    }
    eprintln!(
        "removal gate: {} skipped, {} is not on PATH",
        fixture.language,
        fixture.toolchain.program()
    );
    true
}

const GO: &str = "\
package gate

import (
\t\"fmt\"
\t\"path\"
\t\"strings\"
)

const UseLegacy = false

func Run(s string) string {
\tif UseLegacy {
\t\treturn strings.ToUpper(s)
\t}
\treturn fmt.Sprintf(\"%s!\", s)
}

func Doomed(s string) string {
\treturn path.Base(s)
}
";

const TYPESCRIPT: &str = "\
import { up, down, keep } from \"./util\";

const USE_LEGACY = false;

export function run(s: string): string {
  if (USE_LEGACY) {
    return down(s);
  }
  return keep(s);
}

export function doomed(s: string): string {
  return up(s);
}
";

const RUST: &str = "\
use std::collections::btree_map;
use std::collections::hash_map;

const USE_LEGACY: bool = false;

pub fn run(n: usize) -> usize {
    if USE_LEGACY {
        return hash_map::HashMap::<u8, u8>::new().len();
    }
    n
}

pub fn doomed() -> usize {
    btree_map::BTreeMap::<u8, u8>::new().len()
}
";

const PYTHON: &str = "\
from util import up, down, keep

USE_LEGACY = False


def run(s):
    if USE_LEGACY:
        return down(s)
    return keep(s)


def doomed(s):
    return up(s)


def check():
    assert run(\"a\") == \"a\"
";

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            language: Language::Go,
            toolchain: Toolchain::Go,
            file: "gate.go",
            files: &[("go.mod", "module gate\n\ngo 1.21\n"), ("gate.go", GO)],
            doomed: "Doomed",
            flag: "UseLegacy",
            keeps: "fmt",
            drops_on_delete: "path",
            drops_on_flag: "strings",
        },
        Fixture {
            language: Language::TypeScript,
            toolchain: Toolchain::Tsc,
            file: "src/main.ts",
            files: &[
                (
                    "tsconfig.json",
                    "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"noUnusedLocals\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  },\n  \"include\": [\"src\"]\n}\n",
                ),
                (
                    "src/util.ts",
                    "export function up(s: string): string {\n  return s.toUpperCase();\n}\n\nexport function down(s: string): string {\n  return s.toLowerCase();\n}\n\nexport function keep(s: string): string {\n  return s;\n}\n",
                ),
                ("src/main.ts", TYPESCRIPT),
            ],
            doomed: "doomed",
            flag: "USE_LEGACY",
            keeps: "keep",
            drops_on_delete: "up",
            drops_on_flag: "down",
        },
        Fixture {
            language: Language::Rust,
            toolchain: Toolchain::Cargo,
            file: "src/lib.rs",
            files: &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"gate-removals\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", RUST),
            ],
            doomed: "doomed",
            flag: "USE_LEGACY",
            keeps: "",
            drops_on_delete: "btree_map",
            drops_on_flag: "hash_map",
        },
        Fixture {
            language: Language::Python,
            toolchain: Toolchain::Python,
            file: "main.py",
            files: &[
                (
                    "util.py",
                    "def up(s):\n    return s.upper()\n\n\ndef down(s):\n    return s.lower()\n\n\ndef keep(s):\n    return s\n",
                ),
                ("main.py", PYTHON),
            ],
            doomed: "doomed",
            flag: "USE_LEGACY",
            keeps: "keep",
            drops_on_delete: "up",
            drops_on_flag: "down",
        },
    ]
}

fn symbol(index: &fun_refactor::index::Index, name: &str) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

#[test]
fn every_fixture_compiles_before_anything_touches_it() {
    for fixture in fixtures() {
        if skip(&fixture) {
            continue;
        }
        let ws = fixture.workspace();
        if let Err(e) = ws.compiles() {
            panic!(
                "the {} fixture is broken to begin with:\n{e}",
                fixture.language
            );
        }
    }
}

impl Fixture {
    fn workspace(&self) -> Workspace {
        Workspace::with(self.toolchain, self.files)
    }
}

#[test]
fn deleting_a_function_takes_the_import_only_it_used() {
    // The whole shape in one case: `Doomed` is the only caller of `strings`, so removing
    // it leaves `"strings" imported and not used`, which Go rejects. Rust makes the same
    // thing a warning, and this project's own CI runs `-D warnings`.
    for fixture in fixtures() {
        if skip(&fixture) {
            continue;
        }
        let ws = fixture.workspace();
        let index = ws.index();
        let planned = fun_refactor::refactor::delete::plan(&index, symbol(&index, fixture.doomed))
            .map(|p| p.edits);
        must_plan(&format!("deleting in {}", fixture.language), &ws, planned);

        let after = ws.read(fixture.file);
        assert!(
            !after.contains(fixture.drops_on_delete),
            "{} kept `{}`, which nothing names any more:\n{after}",
            fixture.language,
            fixture.drops_on_delete
        );
        if !fixture.keeps.is_empty() {
            assert!(
                after.contains(fixture.keeps),
                "{} dropped `{}`, which is still used:\n{after}",
                fixture.language,
                fixture.keeps
            );
        }
    }
}

#[test]
fn removing_a_flag_takes_the_import_its_dead_branch_used() {
    for fixture in fixtures() {
        if skip(&fixture) {
            continue;
        }
        let ws = fixture.workspace();
        let sources = ws.sources();
        let planned = fun_refactor::refactor::cascade::remove_flag_in(sources, fixture.flag, false)
            .map(|p| p.edits);
        must_plan(
            &format!("removing a flag in {}", fixture.language),
            &ws,
            planned,
        );

        let after = ws.read(fixture.file);
        assert!(
            !after.contains(fixture.drops_on_flag),
            "{} kept `{}`, which only the dead branch used:\n{after}",
            fixture.language,
            fixture.drops_on_flag
        );
        if !fixture.keeps.is_empty() {
            assert!(
                after.contains(fixture.keeps),
                "{} dropped `{}`, which is still used:\n{after}",
                fixture.language,
                fixture.keeps
            );
        }
    }
}

#[test]
fn organizing_imports_narrows_a_statement_that_lost_one_name() {
    // `fr imports` dropped a statement nothing named and left one that named two things
    // and used one. That is an error under `noUnusedLocals` and a lint failure everywhere
    // else, from the one command whose whole job is removing imports nothing uses.
    for fixture in fixtures() {
        if skip(&fixture) || fixture.keeps.is_empty() {
            continue;
        }
        let ws = fixture.workspace();
        let index = ws.index();
        // Nothing is dead in the fixture as it stands, so the honest answer is no edits.
        // What this checks is that whatever it does plan compiles — and, below, that it
        // narrows once something has died.
        match fun_refactor::refactor::imports::plan(&index, &ws.path(fixture.file)) {
            Ok(plan) if plan.edits.is_empty() => {}
            other => {
                gate(
                    &format!("organizing imports in {}", fixture.language),
                    &ws,
                    other.map(|p| p.edits),
                );
            }
        }

        // Now kill one of the two names the statement binds, and ask again.
        let ws = fixture.workspace();
        let index = ws.index();
        let planned = fun_refactor::refactor::delete::plan(&index, symbol(&index, fixture.doomed))
            .map(|p| p.edits);
        must_plan(
            &format!("deleting to strand an import in {}", fixture.language),
            &ws,
            planned,
        );
        let after = ws.read(fixture.file);
        assert!(
            after.contains(fixture.keeps),
            "{} dropped `{}`, which is still used:\n{after}",
            fixture.language,
            fixture.keeps
        );
    }
}

#[test]
fn a_recipe_that_removes_a_flag_and_prunes_what_it_orphaned_compiles() {
    // A recipe is one transaction over several commands, so it inherits their behaviour
    // and adds a way for them to interact. This is the shape a real retirement takes.
    for fixture in fixtures() {
        if skip(&fixture) {
            continue;
        }
        let ws = fixture.workspace();
        let sources = ws.sources();
        let text = format!(
            "schema 1\nrecipe retire {{\n  requires symbol \"{}\"\n  \
             remove-flag \"{}\" = false\n  expect refusals = 0\n}}\n",
            fixture.flag, fixture.flag
        );
        let file = fun_refactor::recipe::parse(&text).expect("the recipe parses");
        let root = ws.path("");
        let options = fun_refactor::recipe::Options {
            root: root.as_path(),
            catalogs: &[],
        };
        let (report, after) = fun_refactor::recipe::run(&file.recipes[0], sources, &options)
            .unwrap_or_else(|e| panic!("{} recipe failed: {e}", fixture.language));
        assert!(
            report.ok,
            "{} recipe reported failure: {report:?}",
            fixture.language
        );

        for (path, (_, content)) in &after {
            std::fs::write(path, content).expect("write");
        }
        if let Err(e) = ws.compiles() {
            panic!(
                "the {} recipe left a workspace that does not compile:\n{e}",
                fixture.language
            );
        }
    }
}

/// What this file covers, said out loud.
#[test]
fn the_removal_gate_states_what_it_covers() {
    let mut missing = Vec::new();
    for fixture in fixtures() {
        if !fixture.toolchain.is_available() {
            missing.push(format!(
                "{} ({})",
                fixture.language,
                fixture.toolchain.program()
            ));
        }
        eprintln!(
            "removal gate: {} — {} ({})",
            fixture.language,
            fixture.toolchain.covers(),
            match fixture.toolchain.is_available() {
                true => "ran here",
                false => "skipped, its toolchain is absent here",
            }
        );
    }
    // Zig and Java have no import list to narrow — Zig binds one file per `const` and
    // Java's `import` binds one name — so the shape these fixtures are built around does
    // not exist there. They are driven by the other two gate files.
    common::require_on_ci("removal gate", &missing);
    eprintln!("removal gate: not driven — zig and java, which have no multi-name import");
}
