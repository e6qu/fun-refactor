//! Tokens for the recipe language.

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// `[a-z][a-z0-9-]*`, kebab-case, matching the CLI.
    Ident(String),
    Str(String),
    Int(u64),
    Bool(bool),
    Open,
    Close,
    Equals,
    Tilde,
    Bang,
    Arrow,
    Greater,
    Less,
    GreaterEq,
    LessEq,
}

impl Token {
    /// How this token reads in a message, so an error can quote what it found.
    pub fn describe(&self) -> String {
        match self {
            Token::Ident(name) => format!("'{name}'"),
            Token::Str(text) => format!("the string \"{text}\""),
            Token::Int(value) => format!("the number {value}"),
            Token::Bool(value) => format!("'{value}'"),
            Token::Open => "'{'".into(),
            Token::Close => "'}'".into(),
            Token::Equals => "'='".into(),
            Token::Tilde => "'~'".into(),
            Token::Bang => "'!'".into(),
            Token::Arrow => "'=>'".into(),
            Token::Greater => "'>'".into(),
            Token::Less => "'<'".into(),
            Token::GreaterEq => "'>='".into(),
            Token::LessEq => "'<='".into(),
        }
    }
}

/// A token and where it came from, so every message can name a line.
#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
}

/// Split `source` into tokens.
pub fn lex(source: &str) -> Result<Vec<Spanned>> {
    let bytes: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;

    while i < bytes.len() {
        let c = bytes[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // `#` to end of line.
        if c == '#' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
            continue;
        }

        let push = |out: &mut Vec<Spanned>, token: Token| out.push(Spanned { token, line });

        match c {
            '{' => {
                push(&mut out, Token::Open);
                i += 1;
            }
            '}' => {
                push(&mut out, Token::Close);
                i += 1;
            }
            '~' => {
                push(&mut out, Token::Tilde);
                i += 1;
            }
            '!' => {
                push(&mut out, Token::Bang);
                i += 1;
            }
            '=' => {
                if bytes.get(i + 1) == Some(&'>') {
                    push(&mut out, Token::Arrow);
                    i += 2;
                } else {
                    push(&mut out, Token::Equals);
                    i += 1;
                }
            }
            '>' => {
                if bytes.get(i + 1) == Some(&'=') {
                    push(&mut out, Token::GreaterEq);
                    i += 2;
                } else {
                    push(&mut out, Token::Greater);
                    i += 1;
                }
            }
            '<' => {
                if bytes.get(i + 1) == Some(&'=') {
                    push(&mut out, Token::LessEq);
                    i += 2;
                } else {
                    push(&mut out, Token::Less);
                    i += 1;
                }
            }
            // Two string forms, because patterns *are* code and code is full of
            // quotes: `'"%s" % ($X,)'` needs no escaping and stays legible.
            '"' => {
                let (text, next) = quoted(&bytes, i, line)?;
                push(&mut out, Token::Str(text));
                i = next;
            }
            '\'' => {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != '\'' {
                    if bytes[end] == '\n' {
                        bail!("line {line}: a raw string is not closed before the end of the line");
                    }
                    end += 1;
                }
                if end >= bytes.len() {
                    bail!("line {line}: a raw string is not closed");
                }
                push(&mut out, Token::Str(bytes[start..end].iter().collect()));
                i = end + 1;
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let text: String = bytes[start..i].iter().collect();
                push(&mut out, Token::Int(text.parse()?));
            }
            c if c.is_ascii_lowercase() => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_lowercase()
                        || bytes[i].is_ascii_digit()
                        || bytes[i] == '-')
                {
                    i += 1;
                }
                let word: String = bytes[start..i].iter().collect();
                push(
                    &mut out,
                    match word.as_str() {
                        "true" => Token::Bool(true),
                        "false" => Token::Bool(false),
                        _ => Token::Ident(word),
                    },
                );
            }
            other => bail!(
                "line {line}: {other:?} cannot start anything in a recipe. Identifiers are \
                 lower-case and kebab-cased, strings are quoted, and comments start with `#`."
            ),
        }
    }
    Ok(out)
}

/// A `"…"` string, with `\"` and `\\`.
fn quoted(bytes: &[char], at: usize, line: usize) -> Result<(String, usize)> {
    let mut text = String::new();
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            '"' => return Ok((text, i + 1)),
            '\\' => {
                let escaped = bytes.get(i + 1).copied().unwrap_or('"');
                match escaped {
                    '"' | '\\' => text.push(escaped),
                    other => bail!(
                        "line {line}: \\{other} is not an escape here. A quoted string takes \
                         \\\" and \\\\ only; for anything else use a raw string, which has no \
                         escapes at all: '…'"
                    ),
                }
                i += 2;
            }
            '\n' => bail!("line {line}: a string is not closed before the end of the line"),
            other => {
                text.push(other);
                i += 1;
            }
        }
    }
    bail!("line {line}: a string is not closed")
}
