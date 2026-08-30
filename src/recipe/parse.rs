//! The recipe grammar, and the signature table that finishes the job.

use super::lex::{lex, Spanned, Token};
use anyhow::{bail, Result};

/// Every word that can begin a statement.
pub const RESERVED: &[&str] = &[
    "schema",
    "recipe",
    "description",
    "requires",
    "expect",
    "where",
    "on-refusal",
    "limit",
    "allow-empty",
    "to",
    "at",
    "as",
    "rename",
    "delete",
    "move",
    "imports",
    "inline",
    "extract",
    "signature",
    "remove-flag",
    "restructure",
    "rewrite",
    "translate",
];

#[derive(Debug, Clone)]
pub struct File {
    pub schema: u64,
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone)]
pub struct Recipe {
    pub name: String,
    pub description: Option<String>,
    pub requires: Vec<Requirement>,
    pub steps: Vec<Step>,
    pub expects: Vec<Expect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    Language(String),
    Symbol(String),
    Path(String),
}

#[derive(Debug, Clone)]
pub struct Step {
    pub operation: Operation,
    pub selector: Vec<Predicate>,
    pub on_refusal: OnRefusal,
    pub limit: Option<u64>,
    pub allow_empty: bool,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Rename {
        to: String,
    },
    Delete,
    Move {
        to: String,
    },
    Imports,
    Inline {
        call: bool,
    },
    ExtractVariable {
        at: String,
        name: String,
    },
    ExtractFunction {
        at: String,
        name: String,
    },
    Signature {
        change: String,
    },
    RemoveFlag {
        flag: String,
        value: bool,
    },
    Restructure {
        language: String,
        pattern: String,
        template: String,
    },
    Rewrite {
        name: String,
    },
    /// Rewrite the selected files in another language.
    Translate {
        to: String,
    },
}

impl Operation {
    /// What this reads as in a report, the command it composes.
    pub fn describe(&self) -> String {
        match self {
            Operation::Rename { to } => format!("rename to \"{to}\""),
            Operation::Delete => "delete".into(),
            Operation::Move { to } => format!("move to \"{to}\""),
            Operation::Imports => "imports".into(),
            Operation::Inline { call } => {
                format!("inline {}", if *call { "call" } else { "variable" })
            }
            Operation::ExtractVariable { at, name } => {
                format!("extract variable at \"{at}\" as \"{name}\"")
            }
            Operation::ExtractFunction { at, name } => {
                format!("extract function at \"{at}\" as \"{name}\"")
            }
            Operation::Signature { change } => format!("signature \"{change}\""),
            Operation::RemoveFlag { flag, value } => format!("remove-flag \"{flag}\" = {value}"),
            Operation::Restructure {
                language,
                pattern,
                template,
            } => format!("restructure {language} '{pattern}' => '{template}'"),
            Operation::Rewrite { name } => format!("rewrite {name}"),
            Operation::Translate { to } => format!("translate to {to}"),
        }
    }

    /// Does a `where` clause mean anything to this operation?
    fn takes_a_selector(&self) -> bool {
        !matches!(
            self,
            Operation::ExtractVariable { .. }
                | Operation::ExtractFunction { .. }
                | Operation::RemoveFlag { .. }
                | Operation::Restructure { .. }
        )
    }

    /// Why a `where` clause is meaningless here, for the reader who wrote one.
    fn why_no_selector(&self) -> &'static str {
        match self {
            Operation::ExtractVariable { .. } | Operation::ExtractFunction { .. } => {
                "extract takes a range, and a range is a judgement about one specific \
                 piece of code instead of something a predicate can select"
            }
            Operation::RemoveFlag { .. } => {
                "remove-flag acts on the whole workspace: it finds the flag, substitutes \
                 it and follows what that exposes, wherever those are"
            }
            Operation::Restructure { .. } => {
                "restructure's pattern *is* its selector. It rewrites every occurrence \
                 of the shape, and a second way of choosing would contradict the first"
            }
            _ => "",
        }
    }

    /// Reject a step the grammar accepted but the operation cannot mean.
    fn check(&self, step: &Step) -> Result<()> {
        if !self.takes_a_selector() && !step.selector.is_empty() {
            bail!(
                "line {}: `{}` takes no `where` clause. {}",
                step.line,
                self.describe(),
                self.why_no_selector()
            );
        }
        if self.takes_a_selector() && step.selector.is_empty() {
            bail!(
                "line {}: `{}` needs a `where` clause saying what to act on. A step with \
                 no selector would act on everything, which is never what anyone means.",
                step.line,
                self.describe()
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnRefusal {
    /// Abandon the run and write nothing.
    #[default]
    Stop,
    /// Record it, apply the rest, exit non-zero.
    Report,
    /// Record it, apply the rest, exit zero, the refusals were expected.
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    /// `name="x"`, `kind=function`, `in="src/"`.
    Equals { field: String, value: String },
    /// `name~"pre_*"`.
    Glob { field: String, pattern: String },
    /// `unused`, `changed`.
    Flag { field: String, expected: bool },
}

impl Predicate {
    pub fn field(&self) -> &str {
        match self {
            Predicate::Equals { field, .. }
            | Predicate::Glob { field, .. }
            | Predicate::Flag { field, .. } => field,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Predicate::Equals { field, value } => format!("{field}=\"{value}\""),
            Predicate::Glob { field, pattern } => format!("{field}~\"{pattern}\""),
            Predicate::Flag { field, expected } => {
                format!("{}{field}", if *expected { "" } else { "!" })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// `expect no-new unused`
    NoNew(String),
    /// `expect changed > 0 files`
    Changed { how: Comparison, count: u64 },
    /// `expect refusals = 0`
    Refusals { how: Comparison, count: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Gt,
    Lt,
    Ge,
    Le,
}

impl Comparison {
    pub fn holds(self, actual: u64, expected: u64) -> bool {
        match self {
            Comparison::Eq => actual == expected,
            Comparison::Gt => actual > expected,
            Comparison::Lt => actual < expected,
            Comparison::Ge => actual >= expected,
            Comparison::Le => actual <= expected,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Comparison::Eq => "=",
            Comparison::Gt => ">",
            Comparison::Lt => "<",
            Comparison::Ge => ">=",
            Comparison::Le => "<=",
        }
    }
}

/// The schema versions this build understands.
pub const SCHEMA: u64 = 1;

pub fn parse(source: &str) -> Result<File> {
    Parser {
        tokens: lex(source)?,
        at: 0,
    }
    .file()
}

struct Parser {
    tokens: Vec<Spanned>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|t| &t.token)
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.at)
            .or_else(|| self.tokens.last())
            .map(|t| t.line)
            .unwrap_or(1)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).map(|t| t.token.clone());
        self.at += 1;
        token
    }

    /// Is the next token this exact word?
    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(name)) if name == word)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.at_word(word) {
            self.at += 1;
            return true;
        }
        false
    }

    fn want_word(&mut self, word: &str, context: &str) -> Result<()> {
        if self.eat_word(word) {
            return Ok(());
        }
        bail!(
            "line {}: {context} expects `{word}`, found {}",
            self.line(),
            self.found()
        )
    }

    fn found(&self) -> String {
        match self.peek() {
            Some(token) => token.describe(),
            None => "the end of the file".to_string(),
        }
    }

    fn want_string(&mut self, context: &str) -> Result<String> {
        match self.next() {
            Some(Token::Str(text)) => Ok(text),
            _ => {
                self.at -= 1;
                bail!(
                    "line {}: {context} expects a quoted string, found {}",
                    self.line(),
                    self.found()
                )
            }
        }
    }

    fn want_ident(&mut self, context: &str) -> Result<String> {
        match self.next() {
            Some(Token::Ident(name)) => Ok(name),
            _ => {
                self.at -= 1;
                bail!(
                    "line {}: {context} expects a name, found {}",
                    self.line(),
                    self.found()
                )
            }
        }
    }

    fn file(&mut self) -> Result<File> {
        // `schema` first and mandatory: a reader has to be able to refuse a file it
        // does not understand before it has parsed a single step.
        self.want_word("schema", "a recipe file")?;
        let schema = match self.next() {
            Some(Token::Int(value)) => value,
            _ => bail!("line {}: `schema` expects a version number", self.line()),
        };
        if schema != SCHEMA {
            bail!(
                "this file says `schema {schema}` and this build understands schema \
                 {SCHEMA}. Refusing to guess at a version it was not written for."
            );
        }

        let mut recipes = Vec::new();
        while self.peek().is_some() {
            recipes.push(self.recipe()?);
        }
        if recipes.is_empty() {
            bail!("this file declares a schema and contains no recipe");
        }
        Ok(File { schema, recipes })
    }

    fn recipe(&mut self) -> Result<Recipe> {
        self.want_word("recipe", "a file after its schema")?;
        let name = self.want_ident("`recipe`")?;
        match self.next() {
            Some(Token::Open) => {}
            _ => {
                self.at -= 1;
                bail!(
                    "line {}: `recipe {name}` expects `{{`, found {}",
                    self.line(),
                    self.found()
                )
            }
        }

        let mut recipe = Recipe {
            name,
            description: None,
            requires: Vec::new(),
            steps: Vec::new(),
            expects: Vec::new(),
        };

        loop {
            match self.peek() {
                Some(Token::Close) => {
                    self.at += 1;
                    break;
                }
                None => bail!(
                    "line {}: `recipe {}` is not closed",
                    self.line(),
                    recipe.name
                ),
                _ => {}
            }

            if self.eat_word("description") {
                recipe.description = Some(self.want_string("`description`")?);
            } else if self.eat_word("requires") {
                recipe.requires.push(self.requirement()?);
            } else if self.eat_word("expect") {
                recipe.expects.push(self.expectation()?);
            } else {
                recipe.steps.push(self.step()?);
            }
        }

        if recipe.steps.is_empty() {
            bail!(
                "`recipe {}` has no steps, so running it could not change anything",
                recipe.name
            );
        }
        Ok(recipe)
    }

    fn requirement(&mut self) -> Result<Requirement> {
        if self.eat_word("language") {
            return Ok(Requirement::Language(
                self.want_ident("`requires language`")?,
            ));
        }
        if self.eat_word("symbol") {
            return Ok(Requirement::Symbol(self.want_string("`requires symbol`")?));
        }
        if self.eat_word("path") {
            return Ok(Requirement::Path(self.want_string("`requires path`")?));
        }
        bail!(
            "line {}: `requires` takes `language`, `symbol` or `path`, found {}",
            self.line(),
            self.found()
        )
    }

    fn expectation(&mut self) -> Result<Expect> {
        if self.eat_word("no-new") {
            let what = self.want_ident("`expect no-new`")?;
            if !matches!(what.as_str(), "unused" | "duplicates") {
                bail!(
                    "line {}: `expect no-new {what}`. This build can re-run `unused` and \
                     `duplicates` afterwards and compare. It has nothing called '{what}'.",
                    self.line()
                );
            }
            return Ok(Expect::NoNew(what));
        }
        let subject = self.want_ident("`expect`")?;
        let how = self.comparison(&subject)?;
        let count = match self.next() {
            Some(Token::Int(value)) => value,
            _ => {
                self.at -= 1;
                bail!(
                    "line {}: `expect {subject}` expects a number, found {}",
                    self.line(),
                    self.found()
                )
            }
        };
        match subject.as_str() {
            "changed" => {
                // `files` is optional noise that reads better; it changes nothing.
                self.eat_word("files");
                Ok(Expect::Changed { how, count })
            }
            "refusals" => Ok(Expect::Refusals { how, count }),
            other => bail!(
                "line {}: `expect {other}`. The expectations are `no-new`, `changed` \
                 and `refusals`.",
                self.line()
            ),
        }
    }

    fn comparison(&mut self, subject: &str) -> Result<Comparison> {
        let how = match self.peek() {
            Some(Token::Equals) => Comparison::Eq,
            Some(Token::Greater) => Comparison::Gt,
            Some(Token::Less) => Comparison::Lt,
            Some(Token::GreaterEq) => Comparison::Ge,
            Some(Token::LessEq) => Comparison::Le,
            _ => bail!(
                "line {}: `expect {subject}` expects one of = > < >= <=, found {}",
                self.line(),
                self.found()
            ),
        };
        self.at += 1;
        Ok(how)
    }

    fn step(&mut self) -> Result<Step> {
        let line = self.line();
        let operation = self.operation()?;

        let mut step = Step {
            operation: operation.clone(),
            selector: Vec::new(),
            on_refusal: OnRefusal::default(),
            limit: None,
            allow_empty: false,
            line,
        };

        // `where` and the modifiers are order-independent: rejecting `delete on-refusal allow
        // where unused` is a rule nobody remembers and it buys nothing.
        loop {
            if self.eat_word("where") {
                if !step.selector.is_empty() {
                    bail!("line {line}: this step already has a `where` clause");
                }
                step.selector = self.selector()?;
            } else if self.eat_word("on-refusal") {
                step.on_refusal = match self.want_ident("`on-refusal`")?.as_str() {
                    "stop" => OnRefusal::Stop,
                    "report" => OnRefusal::Report,
                    "allow" => OnRefusal::Allow,
                    other => bail!(
                        "line {line}: `on-refusal {other}`. It takes `stop`, `report` or \
                         `allow`. There is deliberately no `ignore`: a refusal is always \
                         in the report, and the only question is the exit code."
                    ),
                };
            } else if self.eat_word("limit") {
                step.limit = match self.next() {
                    Some(Token::Int(value)) => Some(value),
                    _ => {
                        self.at -= 1;
                        bail!(
                            "line {line}: `limit` expects a number, found {}",
                            self.found()
                        )
                    }
                };
            } else if self.eat_word("allow-empty") {
                step.allow_empty = true;
            } else {
                break;
            }
        }

        operation.check(&step)?;
        Ok(step)
    }

    fn operation(&mut self) -> Result<Operation> {
        if self.eat_word("rename") {
            self.want_word("to", "`rename`")?;
            return Ok(Operation::Rename {
                to: self.want_string("`rename to`")?,
            });
        }
        if self.eat_word("delete") {
            return Ok(Operation::Delete);
        }
        if self.eat_word("move") {
            self.want_word("to", "`move`")?;
            return Ok(Operation::Move {
                to: self.want_string("`move to`")?,
            });
        }
        if self.eat_word("imports") {
            return Ok(Operation::Imports);
        }
        if self.eat_word("inline") {
            let what = self.want_ident("`inline`")?;
            return match what.as_str() {
                "variable" => Ok(Operation::Inline { call: false }),
                "call" => Ok(Operation::Inline { call: true }),
                other => bail!(
                    "line {}: `inline {other}`. It takes `variable` or `call`.",
                    self.line()
                ),
            };
        }
        if self.eat_word("extract") {
            let what = self.want_ident("`extract`")?;
            self.want_word("at", "`extract`")?;
            let at = self.want_string("`extract … at`")?;
            self.want_word("as", "`extract`")?;
            let name = self.want_string("`extract … as`")?;
            return match what.as_str() {
                "variable" => Ok(Operation::ExtractVariable { at, name }),
                "function" => Ok(Operation::ExtractFunction { at, name }),
                other => bail!(
                    "line {}: `extract {other}`. It takes `variable` or `function`.",
                    self.line()
                ),
            };
        }
        if self.eat_word("signature") {
            return Ok(Operation::Signature {
                change: self.want_string("`signature`")?,
            });
        }
        if self.eat_word("remove-flag") {
            let flag = self.want_string("`remove-flag`")?;
            match self.next() {
                Some(Token::Equals) => {}
                _ => {
                    self.at -= 1;
                    bail!(
                        "line {}: `remove-flag \"{flag}\"` expects `= true` or `= false`. Give the \
                         value the flag is now assumed to have always had.",
                        self.line()
                    )
                }
            }
            let value = match self.next() {
                Some(Token::Bool(value)) => value,
                _ => {
                    self.at -= 1;
                    bail!(
                        "line {}: `remove-flag \"{flag}\" =` expects `true` or `false`, found {}",
                        self.line(),
                        self.found()
                    )
                }
            };
            return Ok(Operation::RemoveFlag { flag, value });
        }
        if self.eat_word("restructure") {
            let language = self.want_ident("`restructure`")?;
            let pattern = self.want_string("`restructure`")?;
            match self.next() {
                Some(Token::Arrow) => {}
                _ => {
                    self.at -= 1;
                    bail!(
                        "line {}: `restructure` expects `=>` between the shape to find and \
                         what to put there, found {}",
                        self.line(),
                        self.found()
                    )
                }
            }
            let template = self.want_string("`restructure … =>`")?;
            return Ok(Operation::Restructure {
                language,
                pattern,
                template,
            });
        }
        if self.eat_word("rewrite") {
            // The name is not optional: `rewrite where lang=go` parsed happily in the
            // prototype and meant nothing.
            let name = match self.peek() {
                Some(Token::Ident(word)) if !RESERVED.contains(&word.as_str()) => {
                    self.want_ident("`rewrite`")?
                }
                _ => bail!(
                    "line {}: `rewrite` needs the transformation to apply: `invert-if`, \
                     `de-morgan` or `guard-clause`. Found {}",
                    self.line(),
                    self.found()
                ),
            };
            return Ok(Operation::Rewrite { name });
        }

        if self.eat_word("translate") {
            self.want_word("to", "`translate`")?;
            let to = self.want_ident("`translate to`")?;
            // Named here rather than at the first file.
            let Some(language) = crate::lang::Language::from_name(&to) else {
                bail!(
                    "line {}: `translate to {to}`: {to} is not a language this build \
                     knows. The languages are {}.",
                    self.line(),
                    crate::lang::Language::ALL
                        .iter()
                        .map(|l| l.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            };
            if crate::translate::sources_for(language).is_empty() {
                bail!(
                    "line {}: `translate to {to}`: nothing can be written as {to}. {}",
                    self.line(),
                    crate::translate::why_nothing_into(language)
                );
            }
            return Ok(Operation::Translate { to });
        }
        bail!(
            "line {}: {} does not begin a step. The steps are rename, delete, move, \
             imports, inline, extract, signature, remove-flag, restructure, rewrite \
             and translate.",
            self.line(),
            self.found()
        )
    }

    fn selector(&mut self) -> Result<Vec<Predicate>> {
        let mut out = Vec::new();
        loop {
            // A `where` clause runs across as many lines as it needs and ends when a
            // token appears that can only begin something else.
            let negated = matches!(self.peek(), Some(Token::Bang));
            if negated {
                self.at += 1;
            }
            let Some(Token::Ident(field)) = self.peek().cloned() else {
                if negated {
                    bail!(
                        "line {}: `!` expects a predicate after it, found {}",
                        self.line(),
                        self.found()
                    );
                }
                break;
            };
            if RESERVED.contains(&field.as_str()) && !negated {
                break;
            }
            self.at += 1;

            match self.peek() {
                Some(Token::Equals) if !negated => {
                    self.at += 1;
                    let value = self.value(&field)?;
                    out.push(Predicate::Equals { field, value });
                }
                Some(Token::Tilde) if !negated => {
                    self.at += 1;
                    let pattern = self.want_string(&format!("`{field}~`"))?;
                    out.push(Predicate::Glob { field, pattern });
                }
                _ => out.push(Predicate::Flag {
                    field,
                    expected: !negated,
                }),
            }
        }
        if out.is_empty() {
            bail!(
                "line {}: `where` expects at least one predicate, found {}",
                self.line(),
                self.found()
            );
        }
        Ok(out)
    }

    /// A value, which may not be a bare reserved word.
    fn value(&mut self, field: &str) -> Result<String> {
        match self.peek().cloned() {
            Some(Token::Str(text)) => {
                self.at += 1;
                Ok(text)
            }
            Some(Token::Int(value)) => {
                self.at += 1;
                Ok(value.to_string())
            }
            Some(Token::Bool(value)) => {
                self.at += 1;
                Ok(value.to_string())
            }
            Some(Token::Ident(word)) if RESERVED.contains(&word.as_str()) => bail!(
                "line {}: `{field}=` needs a value; found the step keyword '{word}'. \
                 Quote it if that really is the value you meant.",
                self.line()
            ),
            Some(Token::Ident(word)) => {
                self.at += 1;
                Ok(word)
            }
            _ => bail!(
                "line {}: `{field}=` needs a value, found {}",
                self.line(),
                self.found()
            ),
        }
    }
}
