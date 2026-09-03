//! What a recipe may say, for a reader that cannot open `RECIPES.md`.

use super::parse::RESERVED;
use super::run::{FILE_PREDICATES, PREDICATES};
use crate::lang::Language;
use crate::refactor::rewrite::Rewrite;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Verb {
    pub name: &'static str,
    /// The arguments, as the parser wants them.
    pub form: &'static str,
    /// What a selector chooses for this verb: a symbol, a file, a range, the workspace.
    pub acts_on: &'static str,
    /// Whether a `where` clause is required, or rejected.
    pub selector: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Vocabulary {
    pub schema: u32,
    pub requirements: Vec<&'static str>,
    pub verbs: Vec<Verb>,
    pub predicates: Vec<&'static str>,
    /// The subset a step acting on a file may ask.
    pub file_predicates: Vec<&'static str>,
    pub rewrites: Vec<&'static str>,
    pub languages: Vec<&'static str>,
    pub modifiers: Vec<&'static str>,
    pub reserved: Vec<&'static str>,
}

const VERBS: &[Verb] = &[
    Verb {
        name: "rename",
        form: "rename to \"<new name>\"",
        acts_on: "symbol",
        selector: "required",
    },
    Verb {
        name: "delete",
        form: "delete",
        acts_on: "symbol",
        selector: "required",
    },
    Verb {
        name: "move",
        form: "move to \"<path>\"",
        acts_on: "symbol",
        selector: "required",
    },
    Verb {
        name: "imports",
        form: "imports",
        acts_on: "file",
        selector: "required",
    },
    Verb {
        name: "inline",
        form: "inline variable | inline call",
        acts_on: "symbol",
        selector: "required",
    },
    Verb {
        name: "extract",
        form: "extract variable|function at \"<path:l:c-l:c>\" as \"<name>\"",
        acts_on: "range",
        selector: "rejected",
    },
    Verb {
        name: "signature",
        form: "signature \"remove:<i> | move:<from>:<to> | add:<i>:<declaration>:<argument>\"",
        acts_on: "symbol",
        selector: "required",
    },
    Verb {
        name: "remove-flag",
        form: "remove-flag \"<FLAG>\" = true|false",
        acts_on: "workspace",
        selector: "rejected",
    },
    Verb {
        name: "restructure",
        form: "restructure <language> \"<pattern>\" => \"<template>\"",
        acts_on: "workspace",
        selector: "rejected",
    },
    Verb {
        name: "rewrite",
        form: "rewrite <rewrite>",
        acts_on: "file",
        selector: "required",
    },
    Verb {
        name: "translate",
        form: "translate to <language>",
        acts_on: "file",
        selector: "required",
    },
];

pub const REQUIREMENTS: &[&str] = &[
    "language <name>",
    "symbol \"<name>\"",
    "any symbol \"<old name>\" \"<new name>\" [ \"<more names>\" ... ]",
    "path \"<path>\"",
];

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        schema: 1,
        requirements: REQUIREMENTS.to_vec(),
        verbs: VERBS.iter().map(clone_verb).collect(),
        predicates: PREDICATES.to_vec(),
        file_predicates: FILE_PREDICATES.to_vec(),
        rewrites: Rewrite::ALL.iter().map(|r| r.as_str()).collect(),
        languages: Language::ALL.iter().map(|l| l.name()).collect(),
        modifiers: vec![
            "where",
            "limit <n>",
            "on-refusal stop|report|allow",
            "allow-empty",
        ],
        reserved: RESERVED.to_vec(),
    }
}

fn clone_verb(verb: &Verb) -> Verb {
    Verb {
        name: verb.name,
        form: verb.form,
        acts_on: verb.acts_on,
        selector: verb.selector,
    }
}

pub fn render(vocabulary: &Vocabulary) -> String {
    let mut out = format!("schema {}\n\nREQUIREMENTS\n", vocabulary.schema);
    for requirement in &vocabulary.requirements {
        out.push_str(&format!("  {requirement}\n"));
    }
    out.push_str("\nOPERATIONS\n");
    for verb in &vocabulary.verbs {
        out.push_str(&format!(
            "  {:<12} {}\n{:<15}acts on a {}, `where` {}\n",
            verb.name, verb.form, "", verb.acts_on, verb.selector
        ));
    }
    out.push_str("\nPREDICATES, for a step that acts on a symbol\n  ");
    out.push_str(&vocabulary.predicates.join(", "));
    out.push_str("\n\nPREDICATES, for a step that acts on a file\n  ");
    out.push_str(&vocabulary.file_predicates.join(", "));
    out.push_str("\n\nREWRITES\n  ");
    out.push_str(&vocabulary.rewrites.join(", "));
    out.push_str("\n\nMODIFIERS\n  ");
    out.push_str(&vocabulary.modifiers.join(", "));
    out.push_str("\n\nLANGUAGES\n  ");
    out.push_str(&vocabulary.languages.join(", "));
    out.push('\n');
    out
}
