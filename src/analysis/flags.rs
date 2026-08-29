//! A `--flag` a script passes, and the program that declares it.

use crate::index::Index;
use crate::lang::Language;
use anyhow::Result;
use std::path::PathBuf;

/// A flag one file declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub flag: String,
    pub file: PathBuf,
    pub line: usize,
    pub language: Language,
}

/// A flag one file passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passed {
    pub flag: String,
    pub file: PathBuf,
    pub line: usize,
    pub language: Language,
}

/// A flag, everywhere something declares it and everywhere something passes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagUse {
    pub flag: String,
    pub declared: Vec<Declared>,
    pub passed: Vec<Passed>,
}

impl FlagUse {
    /// Is this flag passed by a script and declared by no program here?
    pub fn is_undeclared(&self) -> bool {
        self.declared.is_empty() && !self.passed.is_empty()
    }

    /// Is this flag declared and passed by nothing here?
    pub fn is_unpassed(&self) -> bool {
        self.passed.is_empty() && !self.declared.is_empty()
    }
}

/// Every flag this workspace declares or passes.
pub fn flags(index: &Index) -> Result<Vec<FlagUse>> {
    let mut declared: Vec<Declared> = Vec::new();
    let mut passed: Vec<Passed> = Vec::new();

    for (file, _) in index.files() {
        let Some(language) = crate::lang::detect(file) else {
            continue;
        };
        let Ok(source) = crate::vfs::read_to_string(file) else {
            continue;
        };
        for (line, flag) in declarations(&source, language) {
            declared.push(Declared {
                flag,
                file: file.to_path_buf(),
                line,
                language,
            });
        }
        for (line, flag) in uses(&source, language) {
            passed.push(Passed {
                flag,
                file: file.to_path_buf(),
                line,
                language,
            });
        }
    }

    let mut names: Vec<String> = declared.iter().map(|d| d.flag.clone()).collect();
    names.extend(passed.iter().map(|p| p.flag.clone()));
    names.sort();
    names.dedup();

    Ok(names
        .into_iter()
        .map(|flag| FlagUse {
            declared: declared
                .iter()
                .filter(|d| d.flag == flag)
                .cloned()
                .collect(),
            passed: passed.iter().filter(|p| p.flag == flag).cloned().collect(),
            flag,
        })
        .collect())
}

/// The flags one file declares, with the line each sits on.
fn declarations(source: &str, language: Language) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        let number = at + 1;
        match language {
            Language::Rust => {
                // `#[arg(long = "retention-days")]` names the flag outright.
                if let Some(named) = attribute_value(line, "long") {
                    out.push((number, named));
                    continue;
                }
                // `#[arg(long)]` takes the field's own name, kebab-cased.
                let bare = (line.contains("#[arg(") || line.contains("#[clap("))
                    && has_bare_word(line, "long");
                if bare {
                    if let Some(field) = lines.get(at + 1).and_then(|l| field_name(l)) {
                        out.push((number + 1, field.replace('_', "-")));
                    }
                }
            }
            Language::Go => {
                for word in ["flag.String(", "flag.Int(", "flag.Bool(", "flag.Duration("] {
                    if let Some(named) = first_string_after(line, word) {
                        if is_flag_name(&named) {
                            out.push((number, named));
                        }
                    }
                }
                for word in [
                    "flag.StringVar(",
                    "flag.IntVar(",
                    "flag.BoolVar(",
                    "flag.DurationVar(",
                ] {
                    // The `Var` forms take the destination first, so the name is
                    // the first string rather than the first argument.
                    if let Some(named) = first_string_after(line, word) {
                        if is_flag_name(&named) {
                            out.push((number, named));
                        }
                    }
                }
            }
            Language::Python => {
                if let Some(named) = first_string_after(line, "add_argument(") {
                    if let Some(flag) = named.strip_prefix("--").filter(|f| is_flag_name(f)) {
                        out.push((number, flag.to_string()));
                    }
                }
            }
            Language::TypeScript | Language::Tsx => {
                if let Some(named) = first_string_after(line, ".option(") {
                    // `--retention-days <n>` carries its argument placeholder.
                    let first = named.split_whitespace().next().unwrap_or(&named);
                    // `-r, --retention-days` names the short form first.
                    if let Some(flag) = first.strip_prefix("--") {
                        let flag = flag.trim_end_matches(',');
                        if is_flag_name(flag) {
                            out.push((number, flag.to_string()));
                        }
                    } else if let Some(long) = named.split("--").nth(1) {
                        let long = long.split_whitespace().next().unwrap_or(long);
                        if is_flag_name(long) {
                            out.push((number, long.to_string()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The flags one file passes, with the line each sits on.
fn uses(source: &str, language: Language) -> Vec<(usize, String)> {
    // Only the languages that write a command line.
    if !matches!(
        language,
        Language::Bash | Language::Yaml | Language::Helm | Language::Json
    ) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (at, line) in source.lines().enumerate() {
        for word in line.split_whitespace() {
            let word = word.trim_matches(['"', '\'', ',', ';', '`']);
            let Some(flag) = word.strip_prefix("--") else {
                continue;
            };
            // `--flag=value` names the flag before the `=`.
            let flag = flag.split('=').next().unwrap_or(flag);
            if is_flag_name(flag) {
                out.push((at + 1, flag.to_string()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A flag opens with a letter or a digit: `--` ends the options and `---` heads a
/// YAML document.
fn is_flag_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `long = "retention-days"` inside an attribute.
fn attribute_value(line: &str, key: &str) -> Option<String> {
    if !line.contains("#[arg(") && !line.contains("#[clap(") {
        return None;
    }
    let at = line.find(key)?;
    let rest = line[at + key.len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    quoted(rest).filter(|name| is_flag_name(name))
}

/// Does this attribute name `key` on its own, with no value after it?
fn has_bare_word(line: &str, key: &str) -> bool {
    let Some(at) = line.find(key) else {
        return false;
    };
    let after = line[at + key.len()..].trim_start();
    after.starts_with(',') || after.starts_with(')')
}

/// The name a struct field declares, where this line declares one.
fn field_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.starts_with('#') || line.starts_with("//") {
        return None;
    }
    let (name, _) = line.split_once(':')?;
    let name = name.trim().trim_start_matches("pub ").trim();
    let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    plain.then(|| name.to_string())
}

/// The first string literal after a word, where the word is on this line.
fn first_string_after(line: &str, word: &str) -> Option<String> {
    let at = line.find(word)?;
    quoted(&line[at + word.len()..])
}

/// The text between the first pair of quotes.
fn quoted(text: &str) -> Option<String> {
    let start = text.find(['"', '\''])?;
    let quote = text[start..].chars().next()?;
    let rest = &text[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}
