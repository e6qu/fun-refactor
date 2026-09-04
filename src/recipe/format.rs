use super::{Comparison, Expect, File, OnRefusal, Operation, Predicate, Requirement, Step};
use anyhow::Result;

pub fn source(source: &str) -> Result<String> {
    let comments = comments(source);
    let mut out = file(&super::parse(source)?);
    if !comments.is_empty() {
        out = format!("{}\n\n{out}", comments.join("\n"));
    }
    Ok(out)
}

pub fn file(file: &File) -> String {
    let mut out = format!("schema {}\n", file.schema);
    for recipe in &file.recipes {
        out.push('\n');
        out.push_str(&format!("recipe {} {{\n", recipe.name));

        if let Some(description) = &recipe.description {
            line(&mut out, &format!("description {}", string(description)));
        }
        if recipe.description.is_some() && (!recipe.requires.is_empty() || !recipe.steps.is_empty())
        {
            out.push('\n');
        }

        for item in &recipe.requires {
            line(&mut out, &requirement(item));
        }
        if !recipe.requires.is_empty() && !recipe.steps.is_empty() {
            out.push('\n');
        }

        for item in &recipe.steps {
            line(&mut out, &step(item));
        }
        if !recipe.steps.is_empty() && !recipe.expects.is_empty() {
            out.push('\n');
        }

        for item in &recipe.expects {
            line(&mut out, &format!("expect {}", expect(item)));
        }
        out.push_str("}\n");
    }
    out
}

fn line(out: &mut String, text: &str) {
    out.push_str("  ");
    out.push_str(text);
    out.push('\n');
}

fn requirement(requirement: &Requirement) -> String {
    match requirement {
        Requirement::Language(language) => format!("requires language {language}"),
        Requirement::Path(path) => format!("requires path {}", string(path)),
        Requirement::Symbol {
            names,
            selector: predicates,
            ..
        } => {
            let subject = match names.as_slice() {
                [name] => format!("symbol {}", string(name)),
                names => format!(
                    "any symbol {}",
                    names
                        .iter()
                        .map(|name| string(name))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            };
            format!("requires {subject}{}", selector(predicates))
        }
    }
}

fn step(step: &Step) -> String {
    let mut out = operation(&step.operation);
    out.push_str(&selector(&step.selector));
    match step.on_refusal {
        OnRefusal::Stop => {}
        OnRefusal::Report => out.push_str(" on-refusal report"),
        OnRefusal::Allow => out.push_str(" on-refusal allow"),
    }
    if let Some(limit) = step.limit {
        out.push_str(&format!(" limit {limit}"));
    }
    if step.allow_empty {
        out.push_str(" allow-empty");
    }
    if let Some(id) = &step.id {
        out.push_str(&format!(" id {}", string(id)));
    }
    out
}

fn operation(operation: &Operation) -> String {
    match operation {
        Operation::Rename { to } => format!("rename to {}", string(to)),
        Operation::Delete => "delete".into(),
        Operation::Move { to } => format!("move to {}", string(to)),
        Operation::Imports => "imports".into(),
        Operation::Inline { call } => format!("inline {}", if *call { "call" } else { "variable" }),
        Operation::ExtractVariable { at, name } => {
            format!("extract variable at {} as {}", string(at), string(name))
        }
        Operation::ExtractFunction { at, name } => {
            format!("extract function at {} as {}", string(at), string(name))
        }
        Operation::Signature { change } => format!("signature {}", string(change)),
        Operation::RemoveFlag { flag, value } => format!("remove-flag {} = {value}", string(flag)),
        Operation::Restructure {
            language,
            pattern,
            template,
        } => format!(
            "restructure {language} {} => {}",
            string(pattern),
            string(template)
        ),
        Operation::Rewrite { name } => format!("rewrite {name}"),
        Operation::Translate { to } => format!("translate to {to}"),
    }
}

fn selector(selector: &[Predicate]) -> String {
    match selector.is_empty() {
        true => String::new(),
        false => format!(
            " where {}",
            selector.iter().map(predicate).collect::<Vec<_>>().join(" ")
        ),
    }
}

fn predicate(predicate: &Predicate) -> String {
    match predicate {
        Predicate::Equals { field, value } => format!("{field}={}", string(value)),
        Predicate::Glob { field, pattern } => format!("{field}~{}", string(pattern)),
        Predicate::Flag { field, expected } => {
            format!("{}{field}", if *expected { "" } else { "!" })
        }
    }
}

fn expect(expect: &Expect) -> String {
    match expect {
        Expect::NoNew(what) => format!("no-new {what}"),
        Expect::Matched { how, count } => measure("matched", *how, *count, false),
        Expect::Applied { how, count } => measure("applied", *how, *count, false),
        Expect::Changed { how, count } => measure("changed", *how, *count, true),
        Expect::Refusals { how, count } => measure("refusals", *how, *count, false),
        Expect::Step {
            target,
            measure: kind,
            how,
            count,
            ..
        } => format!(
            "step {} {}",
            target.describe(),
            measure(kind.name(), *how, *count, kind.has_file_unit())
        ),
    }
}

fn measure(name: &str, comparison: Comparison, count: u64, files: bool) -> String {
    format!(
        "{name} {} {count}{}",
        comparison.as_str(),
        if files { " files" } else { "" }
    )
}

fn string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn comments(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let mut quoted = None;
            let mut escaped = false;
            for (at, c) in line.char_indices() {
                match quoted {
                    Some('"') if escaped => escaped = false,
                    Some('"') if c == '\\' => escaped = true,
                    Some(delimiter) if c == delimiter => quoted = None,
                    Some(_) => {}
                    None if matches!(c, '\'' | '"') => quoted = Some(c),
                    None if c == '#' => return Some(line[at..].trim_end().to_string()),
                    None => {}
                }
            }
            None
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::source;
    use crate::recipe::parse;

    #[test]
    fn canonicalizes_layout_spelling_and_modifier_order() {
        let formatted = source(
            "schema 1\nrecipe tidy { delete id \"retire\" allow-empty where kind=function name~'old_*' on-refusal allow\nexpect step \"retire\" changed = 0 }",
        )
        .unwrap();
        assert_eq!(
            formatted,
            "schema 1\n\nrecipe tidy {\n  delete where kind=\"function\" name~\"old_*\" on-refusal allow allow-empty id \"retire\"\n\n  expect step \"retire\" changed = 0 files\n}\n"
        );
        assert_eq!(
            source(&formatted).unwrap(),
            formatted,
            "formatting is stable"
        );
    }

    #[test]
    fn keeps_comments_without_treating_hashes_in_strings_as_comments() {
        let formatted = source(
            "schema 1 # schema note\nrecipe r {\n# before work\n delete where name=\"#not-a-note\" # work note\n}\n",
        )
        .unwrap();
        assert!(formatted.starts_with("# schema note\n# before work\n# work note\n\nschema 1"));
        assert!(formatted.contains("name=\"#not-a-note\""));
        assert_eq!(source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn round_trips_every_recipe_form_without_changing_its_meaning() {
        let source_text = "schema 1\nrecipe every-form {\ndescription 'everything'\nrequires language rust\nrequires symbol \"one\" where kind=function\nrequires any symbol \"first\" \"second\" where in=\"src/\"\nrequires path 'src/*.rs'\nrename to \"new\" where name=old\ndelete where unused\nmove to \"archive/\" where name=old\nimports where changed\ninline variable where name=old\ninline call where name=old\nextract variable at \"src/lib.rs:1:1-1:4\" as \"value\"\nextract function at \"src/lib.rs:1:1-1:4\" as \"helper\"\nsignature \"add:arg:i32\" where name=old\nremove-flag \"OLD\" = false\nrestructure rust 'old($X)' => 'new($X)'\nrewrite guard-clause where lang=go\ntranslate to python where lang=go\nexpect no-new unused\nexpect matched >= 1\nexpect applied > 0\nexpect changed <= 3\nexpect refusals = 0\nexpect step 1 matched < 2\nexpect step \"phase\" changed = 0 files\n}\n";
        let with_id = source_text.replace("rename to \"new\"", "rename to \"new\" id \"phase\"");
        let formatted = source(&with_id).unwrap();
        let parsed = parse(&formatted).expect("the canonical output parses");
        let recipe = &parsed.recipes[0];
        assert_eq!(recipe.requires.len(), 4);
        assert_eq!(recipe.steps.len(), 13, "every operation survived");
        assert_eq!(recipe.expects.len(), 7, "every expectation survived");
        assert_eq!(
            source(&formatted).unwrap(),
            formatted,
            "the output is stable"
        );
    }

    #[test]
    fn the_self_hosted_recipe_stays_in_canonical_layout() {
        let recipe = include_str!("../../recipes/self-hosted-transaction.recipe");
        assert_eq!(source(recipe).unwrap(), recipe);
    }
}
