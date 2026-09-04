use super::{Comparison, Expect, File, OnRefusal, Operation, Predicate, Requirement, Step};
use anyhow::Result;

pub fn source(source: &str) -> Result<String> {
    Ok(with_comments(
        render(&super::parse(source)?),
        comments(source),
    ))
}

pub fn file(file: &File) -> String {
    text(render(file))
}

struct RenderedLine {
    text: String,
    source_line: Option<usize>,
    indent: usize,
}

fn code(text: impl Into<String>, source_line: usize, indent: usize) -> RenderedLine {
    RenderedLine {
        text: format!("{}{}", " ".repeat(indent), text.into()),
        source_line: Some(source_line),
        indent,
    }
}

fn blank() -> RenderedLine {
    RenderedLine {
        text: String::new(),
        source_line: None,
        indent: 0,
    }
}

fn render(file: &File) -> Vec<RenderedLine> {
    let mut out = vec![code(format!("schema {}", file.schema), file.schema_line, 0)];
    for recipe in &file.recipes {
        out.push(blank());
        out.push(code(format!("recipe {} {{", recipe.name), recipe.line, 0));

        if let Some(description) = &recipe.description {
            out.push(code(
                format!("description {}", string(description)),
                recipe
                    .description_line
                    .expect("a description has a source line"),
                2,
            ));
        }
        if recipe.description.is_some() && (!recipe.requires.is_empty() || !recipe.steps.is_empty())
        {
            out.push(blank());
        }

        for item in &recipe.requires {
            out.push(code(requirement(item), requirement_line(item), 2));
        }
        if !recipe.requires.is_empty() && !recipe.steps.is_empty() {
            out.push(blank());
        }

        for item in &recipe.steps {
            out.push(code(step(item), item.line, 2));
        }
        if !recipe.steps.is_empty() && !recipe.expects.is_empty() {
            out.push(blank());
        }

        for (item, line) in recipe.expects.iter().zip(&recipe.expect_lines) {
            out.push(code(format!("expect {}", expect(item)), *line, 2));
        }
        out.push(code("}", recipe.end_line, 0));
    }
    out
}

fn text(lines: Vec<RenderedLine>) -> String {
    lines
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn requirement(requirement: &Requirement) -> String {
    match requirement {
        Requirement::Language { name, .. } => format!("requires language {name}"),
        Requirement::Path { path, .. } => format!("requires path {}", string(path)),
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

fn requirement_line(requirement: &Requirement) -> usize {
    match requirement {
        Requirement::Language { line, .. }
        | Requirement::Symbol { line, .. }
        | Requirement::Path { line, .. } => *line,
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

struct Comment {
    line: usize,
    text: String,
}

fn comments(source: &str) -> Vec<Comment> {
    source
        .lines()
        .enumerate()
        .filter_map(|(number, line)| {
            let mut quoted = None;
            let mut escaped = false;
            for (at, c) in line.char_indices() {
                match quoted {
                    Some('"') if escaped => escaped = false,
                    Some('"') if c == '\\' => escaped = true,
                    Some(delimiter) if c == delimiter => quoted = None,
                    Some(_) => {}
                    None if matches!(c, '\'' | '"') => quoted = Some(c),
                    None if c == '#' => {
                        return Some(Comment {
                            line: number + 1,
                            text: line[at..].trim_end().to_string(),
                        })
                    }
                    None => {}
                }
            }
            None
        })
        .collect()
}

fn with_comments(mut lines: Vec<RenderedLine>, comments: Vec<Comment>) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    for index in 0..lines.len() {
        let Some(source_line) = lines[index].source_line else {
            out.push(lines[index].text.clone());
            continue;
        };
        while comments
            .get(at)
            .is_some_and(|comment| comment.line < source_line)
        {
            let indent = if lines[index].text.trim() == "}" {
                lines[..index]
                    .iter()
                    .rev()
                    .find(|line| !line.text.is_empty())
                    .map_or(0, |line| line.indent)
            } else {
                lines[index].indent
            };
            out.push(format!("{}{}", " ".repeat(indent), comments[at].text));
            at += 1;
        }
        while comments
            .get(at)
            .is_some_and(|comment| comment.line == source_line)
        {
            lines[index].text.push(' ');
            lines[index].text.push_str(&comments[at].text);
            at += 1;
        }
        out.push(lines[index].text.clone());
    }
    for comment in &comments[at..] {
        out.push(comment.text.clone());
    }
    out.join("\n") + "\n"
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
    fn keeps_comments_beside_the_directives_they_explain() {
        let formatted = source(
            "schema 1 # schema note\nrecipe r {\n# before work\n delete where name=\"#not-a-note\" # work note\n}\n# after recipe\n",
        )
        .unwrap();
        assert_eq!(
            formatted,
            "schema 1 # schema note\n\nrecipe r {\n  # before work\n  delete where name=\"#not-a-note\" # work note\n}\n# after recipe\n"
        );
        assert_eq!(source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn preserves_comments_at_every_recipe_boundary() {
        let formatted = source(
            "# file\nschema 1\n\n# recipe\nrecipe r { # opening\n\n# description\ndescription \"desc\" # description tail\n\n# requirement\nrequires language rust # requirement tail\n\n# step\ndelete where unused # step tail\n\n# expectation\nexpect refusals = 0 # expectation tail\n\n# closing\n} # closing tail\n",
        )
        .unwrap();
        let lines = formatted.lines().collect::<Vec<_>>();
        for expected in [
            "# file",
            "schema 1",
            "# recipe",
            "recipe r { # opening",
            "  # description",
            "  description \"desc\" # description tail",
            "  # requirement",
            "  requires language rust # requirement tail",
            "  # step",
            "  delete where unused # step tail",
            "  # expectation",
            "  expect refusals = 0 # expectation tail",
            "  # closing",
            "} # closing tail",
        ] {
            assert!(
                lines.contains(&expected),
                "missing {expected:?}: {formatted}"
            );
        }
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
