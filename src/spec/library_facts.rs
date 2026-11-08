use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct LibraryFacts {
    pub operations: Vec<String>,
    pub facts: Vec<String>,
    pub omitted: usize,
    pub source: &'static str,
}

pub fn operations(spec_source: &str) -> Vec<String> {
    let name = Regex::new(r"\b[A-Z][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_']*)+").unwrap();
    let mut found = BTreeSet::new();
    for line in spec_source.lines() {
        let code = line.split("--").next().unwrap_or_default();
        if code.trim_start().starts_with("import ") {
            continue;
        }
        for matched in name.find_iter(code) {
            if !matched.as_str().starts_with("FrSpecs.") {
                found.insert(matched.as_str().to_owned());
            }
        }
    }
    found.into_iter().collect()
}

pub fn imports(spec_source: &str) -> Vec<String> {
    spec_source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .flat_map(|names| names.split_whitespace().map(str::to_owned))
        .collect()
}

pub fn lookup(package: &Path, spec_source: &str) -> Result<LibraryFacts> {
    let output = Command::new("lean")
        .arg("--print-prefix")
        .current_dir(package)
        .output()
        .context("find the Lean toolchain for library facts")?;
    anyhow::ensure!(
        output.status.success(),
        "lean --print-prefix failed for library facts"
    );
    let prefix = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let roots = vec![
        (prefix.join("lib/lean"), prefix.join("src/lean")),
        (package.join(".lake/build/lib/lean"), package.to_path_buf()),
    ];
    let operations = operations(spec_source);
    let facts = collect(&roots, &imports(spec_source), &operations)?;
    Ok(LibraryFacts {
        operations,
        facts,
        omitted: 0,
        source: "Lean .ilean indexes of the model's imports; statements as written in library source; @[simp] facts omitted",
    })
}

pub fn collect(
    roots: &[(PathBuf, PathBuf)],
    imports: &[String],
    operations: &[String],
) -> Result<Vec<String>> {
    let wanted = operations
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut pending = imports.to_vec();
    let mut facts = BTreeMap::new();
    while let Some(module) = pending.pop() {
        if !seen.insert(module.clone()) {
            continue;
        }
        let relative = module.replace('.', "/");
        let Some((index, source)) = roots.iter().find_map(|(library, sources)| {
            let index = library.join(format!("{relative}.ilean"));
            index
                .is_file()
                .then(|| (index, sources.join(format!("{relative}.lean"))))
        }) else {
            continue;
        };
        let index: Value = serde_json::from_slice(&std::fs::read(&index)?)?;
        for imported in index["directImports"].as_array().into_iter().flatten() {
            if let Some(name) = imported[0].as_str() {
                pending.push(name.to_owned());
            }
        }
        let Ok(text) = std::fs::read_to_string(&source) else {
            continue;
        };
        let lines = text.lines().collect::<Vec<_>>();
        let references = index["references"].as_object().into_iter().flatten();
        for (key, reference) in references {
            let constant: Value = serde_json::from_str(key)?;
            if !constant["c"]["n"]
                .as_str()
                .is_some_and(|name| wanted.contains(name))
            {
                continue;
            }
            for usage in reference["usages"].as_array().into_iter().flatten() {
                let Some(name) = usage.get(4).and_then(Value::as_str) else {
                    continue;
                };
                if facts.contains_key(name) {
                    continue;
                }
                let at = (number(&usage[0]), number(&usage[1]));
                if let Some(fact) = statement(&lines, &index["decls"][name], name, at) {
                    facts.insert(name.to_owned(), fact);
                }
            }
        }
    }
    let mut facts = facts.into_values().collect::<Vec<_>>();
    facts.sort_by(|left, right| (left.len(), left).cmp(&(right.len(), right)));
    Ok(facts)
}

fn number(value: &Value) -> usize {
    value.as_u64().unwrap_or(u64::MAX) as usize
}

fn byte_column(line: &str, utf16: usize) -> Option<usize> {
    let mut units = 0;
    for (offset, character) in line.char_indices() {
        if units >= utf16 {
            return Some(offset);
        }
        units += character.len_utf16();
    }
    (units >= utf16).then_some(line.len())
}

fn statement(lines: &[&str], range: &Value, name: &str, usage: (usize, usize)) -> Option<String> {
    let range = range.as_array()?.iter().map(number).collect::<Vec<_>>();
    let &[start, _, end, _, name_line, name_column, ..] = range.as_slice() else {
        return None;
    };
    let name_start = byte_column(lines.get(name_line)?, name_column)?;
    let prefix = lines.get(start..name_line)?.join("\n") + "\n" + &lines[name_line][..name_start];
    let attribute = Regex::new(r"@\[[^\]]*\b(simp|deprecated)\b").unwrap();
    if !Regex::new(r"\btheorem\s*$").unwrap().is_match(&prefix)
        || Regex::new(r"\bprivate\b").unwrap().is_match(&prefix)
        || attribute.is_match(&prefix)
    {
        return None;
    }
    let mut text = lines[name_line][name_start..].to_owned();
    let mut boundary = None;
    for (index, line) in lines.iter().enumerate().take(end + 1).skip(name_line) {
        let first = index == name_line;
        let piece = if first {
            &lines[name_line][name_start..]
        } else {
            *line
        };
        if !first && piece.trim_start().starts_with('|') {
            boundary = Some((index, 0));
            break;
        }
        if let Some(offset) = piece.find(":=") {
            let column = if first { name_start + offset } else { offset };
            if !first {
                text.push(' ');
                text.push_str(&piece[..offset]);
            } else {
                text.truncate(offset);
            }
            boundary = Some((index, column));
            break;
        }
        if !first {
            text.push(' ');
            text.push_str(piece);
        }
    }
    let (boundary_line, boundary_column) = boundary?;
    let usage_column = byte_column(lines.get(usage.0)?, usage.1)?;
    if (usage.0, usage_column) >= (boundary_line, boundary_column) {
        return None;
    }
    let mut words = text.split_whitespace();
    words.next()?;
    let rest = words.collect::<Vec<_>>().join(" ");
    Some(format!("{name} {rest}").trim_end().to_owned())
}

pub fn fit(facts: &mut LibraryFacts, available: usize) {
    let mut used = 0;
    let mut kept = 0;
    for fact in &facts.facts {
        let cost = serde_json::to_string(fact).map_or(usize::MAX, |text| text.len() + 8);
        if used + cost > available.min(3072) {
            break;
        }
        used += cost;
        kept += 1;
    }
    facts.omitted = facts.facts.len() - kept;
    facts.facts.truncate(kept);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_facts_come_from_the_lean_index_of_imported_modules() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("lib");
        let sources = root.path().join("src");
        std::fs::create_dir_all(library.join("Lib")).unwrap();
        std::fs::create_dir_all(sources.join("Lib")).unwrap();
        std::fs::write(
            sources.join("Lib/Div.lean"),
            [
                "namespace Int",
                "protected theorem tdiv_nonneg {a b : Int} (Ha : 0 ≤ a)",
                "    (Hb : 0 ≤ b) : 0 ≤ a.tdiv b := by",
                "  exact helper",
                "@[simp] theorem tdiv_zero (a : Int) : a.tdiv 0 = 0 := rfl",
                "theorem uses_in_proof (a : Int) : a = a := by",
                "  have := a.tdiv a",
                "  rfl",
                "private theorem hidden (a : Int) : a.tdiv a = a.tdiv a := rfl",
                "end Int",
            ]
            .join("\n")
                + "\n",
        )
        .unwrap();
        let tdiv = r#"{"c":{"m":"Lib.Base","n":"Int.tdiv"}}"#;
        let index = serde_json::json!({
            "version": 5, "module": "Lib.Div", "directImports": [["Lib.Missing", false, false, false]],
            "decls": {
                "Int.tdiv_nonneg": [1, 0, 3, 14, 1, 18, 1, 29, 0, 0, 0, 0],
                "Int.tdiv_zero": [4, 0, 4, 54, 4, 16, 4, 25],
                "Int.uses_in_proof": [5, 0, 7, 5, 5, 8, 5, 21],
                "Int.hidden": [8, 0, 8, 60, 8, 16, 8, 22]
            },
            "references": {tdiv: {"definition": null, "usages": [
                [2, 25, 2, 29, "Int.tdiv_nonneg"], [4, 40, 4, 44, "Int.tdiv_zero"],
                [6, 12, 6, 16, "Int.uses_in_proof"], [8, 37, 8, 41, "Int.hidden"], [9, 0, 9, 4]
            ]}}
        });
        std::fs::write(library.join("Lib/Div.ilean"), index.to_string()).unwrap();
        let facts = collect(
            &[(library, sources)],
            &["Lib.Div".into()],
            &["Int.tdiv".into()],
        )
        .unwrap();
        assert_eq!(
            facts,
            vec!["Int.tdiv_nonneg {a b : Int} (Ha : 0 ≤ a) (Hb : 0 ≤ b) : 0 ≤ a.tdiv b"]
        );
        let spec = "prelude\nimport Init.Data.Int.DivMod.Lemmas\n-- Nat.add only in a comment\ndef m (a b : Int) : Int := Int.tdiv a b + FrSpecs.x\n";
        assert_eq!(operations(spec), vec!["Int.tdiv"]);
        assert_eq!(imports(spec), vec!["Init.Data.Int.DivMod.Lemmas"]);
        let mut fitted = LibraryFacts {
            operations: vec![],
            facts: vec!["a".repeat(100), "b".repeat(100)],
            omitted: 0,
            source: "",
        };
        fit(&mut fitted, 150);
        assert_eq!((fitted.facts.len(), fitted.omitted), (1, 1));
    }
}
