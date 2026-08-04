//! Writing the IR out as a language.
//!
//! Each writer is idiomatic for its target rather than a transliteration: a record
//! becomes a Rust `struct` with an `impl` block, a Python `@dataclass`, a Go `struct`
//! with methods beside it, a TypeScript `class`. Naming follows the target's
//! convention — `snake_case` in Rust and Python, `camelCase` in TypeScript,
//! `PascalCase` for exported Go — because a file that reads as a foreign language
//! wearing a costume is not a file anyone will keep.
//!
//! **The signature is the contract.** Parameter names, their order, their types and
//! the return type are carried exactly; only the spelling changes. Where a type has no
//! counterpart it is written through by name and counted in the fidelity report,
//! because silently substituting a type is how a signature stops meaning what it said.
//!
//! Anything the reader could not translate is emitted as a comment holding the
//! original source, under a marker. The result is a file you finish, not one you have
//! to diff to discover what is missing.

use super::ir::*;
use crate::lang::Language;
use anyhow::{bail, Result};

/// The marker every carried-over fragment is written under.
pub const MARKER: &str = "fun-refactor: not translated";

pub fn write(language: Language, module: &Module) -> Result<(String, Fidelity)> {
    let mut out = Out::new(language);
    match language {
        Language::Rust => rust(&mut out, module),
        Language::Python => python(&mut out, module),
        Language::Go => go(&mut out, module),
        Language::TypeScript | Language::Tsx => typescript(&mut out, module),
        other => bail!(
            "there is no writer for {other}: it has no functions or records to write \
             these into"
        ),
    }
    Ok((out.finish(), out.fidelity))
}

struct Out {
    language: Language,
    text: String,
    indent: usize,
    fidelity: Fidelity,
}

impl Out {
    fn new(language: Language) -> Self {
        Out {
            language,
            text: String::new(),
            indent: 0,
            fidelity: Fidelity::default(),
        }
    }

    fn line(&mut self, text: &str) {
        if !text.is_empty() {
            for _ in 0..self.indent {
                self.text.push_str("    ");
            }
            self.text.push_str(text);
        }
        self.text.push('\n');
    }

    fn blank(&mut self) {
        if !self.text.ends_with("\n\n") && !self.text.is_empty() {
            self.text.push('\n');
        }
    }

    fn open(&mut self) {
        self.indent += 1;
    }

    fn close(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    /// Record a fragment that had no counterpart, and say where it came from.
    fn carried(&mut self, what: &Unsupported) {
        self.fidelity.carried_verbatim += 1;
        self.fidelity.notes.push(format!(
            "line {}: {} carried over unchanged",
            what.line, what.construct
        ));
    }

    fn finish(&self) -> String {
        self.text.clone()
    }

    fn comment(&self, text: &str) -> String {
        match self.language {
            Language::Python => format!("# {text}"),
            _ => format!("// {text}"),
        }
    }
}

/// Write a carried-over fragment as a comment, whole, so nothing is lost.
fn carry(out: &mut Out, what: &Unsupported) {
    out.carried(what);
    let header = out.comment(&format!(
        "{MARKER}: {} from line {}",
        what.construct, what.line
    ));
    out.line(&header);
    for line in what.source.lines() {
        let commented = out.comment(line);
        out.line(&commented);
    }
}

// ------------------------------------------------------------------ conventions

/// `snake_case`, for Rust and Python.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `camelCase`, for TypeScript and unexported Go.
fn camel(name: &str) -> String {
    // SCREAMING_SNAKE has to be lowered before it is re-cased, or `MIN_CELSIUS`
    // becomes `MINCELSIUS` rather than `minCelsius`.
    let screaming = name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric());
    let source = if screaming {
        name.to_lowercase()
    } else {
        name.to_string()
    };

    let mut out = String::with_capacity(source.len());
    let mut upper_next = false;
    for (i, c) in source.chars().enumerate() {
        if c == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else if i == 0 {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `PascalCase`, for type names everywhere and exported Go.
fn pascal(name: &str) -> String {
    let camel = camel(name);
    let mut chars = camel.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => camel,
    }
}

// ------------------------------------------------------------------------- Rust

fn rust(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("//! {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/// {line}"));
                }
                let ty =
                    c.ty.as_ref()
                        .map(rust_type)
                        .unwrap_or_else(|| "&str".to_string());
                let visibility = if c.exported { "pub " } else { "" };
                let value = rust_expr(out, &c.value);
                out.line(&format!(
                    "{visibility}const {}: {ty} = {value};",
                    c.name.to_uppercase()
                ));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                for line in &r.doc {
                    out.line(&format!("/// {line}"));
                }
                let visibility = if r.exported { "pub " } else { "" };
                out.line(&format!("{visibility}struct {} {{", pascal(&r.name)));
                out.open();
                for f in &r.fields {
                    for line in &f.doc {
                        out.line(&format!("/// {line}"));
                    }
                    let ty =
                        f.ty.as_ref()
                            .map(rust_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    let field_visibility = if f.exported { "pub " } else { "" };
                    out.line(&format!("{field_visibility}{}: {ty},", snake(&f.name)));
                }
                out.close();
                out.line("}");
                out.fidelity.records += 1;
                out.blank();

                if !r.methods.is_empty() {
                    // Rust declares methods apart from the type, which is what the
                    // record's method list becomes.
                    out.line(&format!("impl {} {{", pascal(&r.name)));
                    out.open();
                    for m in &r.methods {
                        rust_function(out, m, true);
                    }
                    out.close();
                    out.line("}");
                    out.blank();
                }
            }
            Item::Function(f) => {
                rust_function(out, f, false);
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn rust_function(out: &mut Out, f: &Function, method: bool) {
    for line in &f.doc {
        out.line(&format!("/// {line}"));
    }
    if f.is_async {
        out.line(&out.comment("declared async in the source"));
    }
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push("&self".to_string());
    }
    let mut foreign = false;
    for p in &f.params {
        let ty = match &p.ty {
            Some(t) => {
                if is_foreign(t) {
                    foreign = true;
                }
                rust_type(t)
            }
            None => {
                foreign = true;
                unknown(out, &p.name)
            }
        };
        params.push(format!("{}: {ty}", snake(&p.name)));
    }
    let returns = match &f.returns {
        Some(Type::Unit) | None => String::new(),
        Some(t) => {
            if is_foreign(t) {
                foreign = true;
            }
            format!(" -> {}", rust_type(t))
        }
    };
    let visibility = if f.exported { "pub " } else { "" };
    out.line(&format!(
        "{visibility}fn {}({}){returns} {{",
        snake(&f.name),
        params.join(", ")
    ));
    out.open();
    rust_block(out, &f.body);
    out.close();
    out.line("}");

    out.fidelity.functions += 1;
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else {
        out.fidelity.signatures_complete += 1;
    }
}

fn rust_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line("todo!()");
        return;
    }
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_default();
                out.line(&format!("return {text};"));
            }
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| format!(": {}", rust_type(t)))
                    .unwrap_or_default();
                let m = if *mutable { "mut " } else { "" };
                let v = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_else(|| "Default::default()".to_string());
                out.line(&format!("let {m}{}{annotation} = {v};", snake(name)));
            }
            Stmt::Assign { target, value } => {
                let t = rust_expr(out, target);
                let v = rust_expr(out, value);
                out.line(&format!("{t} = {v};"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = rust_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                rust_block(out, then);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    rust_block(out, otherwise);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::While { condition, body } => {
                let c = rust_expr(out, condition);
                out.line(&format!("while {c} {{"));
                out.open();
                rust_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = rust_expr(out, iterable);
                out.line(&format!("for {} in {it} {{", snake(binding)));
                out.open();
                rust_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = rust_expr(out, e);
                out.line(&format!("{text};"));
            }
            Stmt::Break => out.line("break;"),
            Stmt::Continue => out.line("continue;"),
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
}

fn rust_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "()".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "i64".to_string(),
        Type::Float => "f64".to_string(),
        Type::String => "String".to_string(),
        Type::List(inner) => format!("Vec<{}>", rust_type(inner)),
        Type::Map(k, v) => format!(
            "std::collections::HashMap<{}, {}>",
            rust_type(k),
            rust_type(v)
        ),
        Type::Optional(inner) => format!("Option<{}>", rust_type(inner)),
        Type::Named { name, args } => generic(name, args, "<", ">", "::", rust_type),
    }
}

fn rust_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "None".to_string(),
        Expr::Name(n) => snake(n),
        Expr::Field { of, name } => format!("{}.{}", rust_expr(out, of), name),
        Expr::Index { of, index } => {
            format!("{}[{}]", rust_expr(out, of), rust_expr(out, index))
        }
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            format!("{}({})", rust_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            rust_expr(out, left),
            op.c_like(),
            rust_expr(out, right)
        ),
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", rust_expr(out, operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| rust_expr(out, i)).collect();
            format!("vec![{}]", rendered.join(", "))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("todo!(\"{}: {}\")", MARKER, u.source.replace('"', "'"))
        }
    }
}

// ----------------------------------------------------------------------- Python

fn python(out: &mut Out, module: &Module) {
    if !module.doc.is_empty() {
        out.line("\"\"\"");
        for line in &module.doc {
            out.line(line);
        }
        out.line("\"\"\"");
        out.blank();
    }

    let needs_dataclass = module
        .items
        .iter()
        .any(|i| matches!(i, Item::Record(r) if !r.fields.is_empty()));
    if needs_dataclass {
        out.line("from dataclasses import dataclass");
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("# {line}"));
                }
                let annotation =
                    c.ty.as_ref()
                        .map(|t| format!(": {}", python_type(t)))
                        .unwrap_or_default();
                let value = python_expr(out, &c.value);
                out.line(&format!("{}{annotation} = {value}", c.name.to_uppercase()));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                // A record with fields is a dataclass; one with only methods is a
                // plain class, because `@dataclass` on it would say nothing.
                if !r.fields.is_empty() {
                    out.line("@dataclass");
                }
                out.line(&format!("class {}:", pascal(&r.name)));
                out.open();
                if !r.doc.is_empty() {
                    out.line("\"\"\"");
                    for line in &r.doc {
                        out.line(line);
                    }
                    out.line("\"\"\"");
                    out.blank();
                }
                for f in &r.fields {
                    for line in &f.doc {
                        out.line(&format!("# {line}"));
                    }
                    let annotation =
                        f.ty.as_ref()
                            .map(python_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    out.line(&format!("{}: {annotation}", snake(&f.name)));
                }
                if r.fields.is_empty() && r.methods.is_empty() && r.doc.is_empty() {
                    out.line("pass");
                }
                for m in &r.methods {
                    out.blank();
                    python_function(out, m, true);
                }
                out.close();
                out.fidelity.records += 1;
                out.blank();
            }
            Item::Function(f) => {
                python_function(out, f, false);
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn python_function(out: &mut Out, f: &Function, method: bool) {
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push("self".to_string());
    }
    let mut foreign = false;
    for p in &f.params {
        let annotation = match &p.ty {
            Some(t) => {
                if is_foreign(t) {
                    foreign = true;
                }
                format!(": {}", python_type(t))
            }
            None => {
                foreign = true;
                String::new()
            }
        };
        let default = p
            .default
            .as_ref()
            .map(|d| format!(" = {}", python_expr(out, d)))
            .unwrap_or_default();
        params.push(format!("{}{annotation}{default}", snake(&p.name)));
    }
    let returns = match &f.returns {
        None => String::new(),
        Some(Type::Unit) => " -> None".to_string(),
        Some(t) => {
            if is_foreign(t) {
                foreign = true;
            }
            format!(" -> {}", python_type(t))
        }
    };
    let prefix = if f.is_async { "async def" } else { "def" };
    out.line(&format!(
        "{prefix} {}({}){returns}:",
        snake(&f.name),
        params.join(", ")
    ));
    out.open();
    if !f.doc.is_empty() {
        if f.doc.len() == 1 {
            out.line(&format!("\"\"\"{}\"\"\"", f.doc[0]));
        } else {
            out.line("\"\"\"");
            for line in &f.doc {
                out.line(line);
            }
            out.line("\"\"\"");
        }
    }
    python_block(out, &f.body);
    out.close();

    out.fidelity.functions += 1;
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else {
        out.fidelity.signatures_complete += 1;
    }
}

fn python_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line("raise NotImplementedError");
        return;
    }
    let mut wrote = false;
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", python_expr(out, v)))
                    .unwrap_or_default();
                out.line(&format!("return{text}"));
                wrote = true;
            }
            Stmt::Let {
                name, ty, value, ..
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| format!(": {}", python_type(t)))
                    .unwrap_or_default();
                let v = value
                    .as_ref()
                    .map(|v| python_expr(out, v))
                    .unwrap_or_else(|| "None".to_string());
                out.line(&format!("{}{annotation} = {v}", snake(name)));
                wrote = true;
            }
            Stmt::Assign { target, value } => {
                let t = python_expr(out, target);
                let v = python_expr(out, value);
                out.line(&format!("{t} = {v}"));
                wrote = true;
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = python_expr(out, condition);
                out.line(&format!("if {c}:"));
                out.open();
                python_block(out, then);
                out.close();
                if !otherwise.is_empty() {
                    // `else: if ...` is written `elif` when that is all it is.
                    if otherwise.len() == 1 {
                        if let Stmt::If { .. } = &otherwise[0] {
                            out.line("else:");
                            out.open();
                            python_block(out, otherwise);
                            out.close();
                            wrote = true;
                            continue;
                        }
                    }
                    out.line("else:");
                    out.open();
                    python_block(out, otherwise);
                    out.close();
                }
                wrote = true;
            }
            Stmt::While { condition, body } => {
                let c = python_expr(out, condition);
                out.line(&format!("while {c}:"));
                out.open();
                python_block(out, body);
                out.close();
                wrote = true;
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = python_expr(out, iterable);
                out.line(&format!("for {} in {it}:", snake(binding)));
                out.open();
                python_block(out, body);
                out.close();
                wrote = true;
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = python_expr(out, e);
                out.line(&text);
                wrote = true;
            }
            Stmt::Break => {
                out.line("break");
                wrote = true;
            }
            Stmt::Continue => {
                out.line("continue");
                wrote = true;
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
    // A body that is only carried-over comments still needs a statement to be Python.
    if !wrote {
        out.line("raise NotImplementedError");
    }
}

fn python_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "None".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float".to_string(),
        Type::String => "str".to_string(),
        Type::List(inner) => format!("list[{}]", python_type(inner)),
        Type::Map(k, v) => format!("dict[{}, {}]", python_type(k), python_type(v)),
        Type::Optional(inner) => format!("{} | None", python_type(inner)),
        // Python spells generics with brackets, which is why the arguments are kept
        // apart: `Result<(), String>` written literally is not a Python annotation.
        Type::Named { name, args } => generic(name, args, "[", "]", ".", python_type),
    }
}

fn python_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => if *v { "True" } else { "False" }.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "None".to_string(),
        Expr::Name(n) => snake(n),
        Expr::Field { of, name } => format!("{}.{}", python_expr(out, of), name),
        Expr::Index { of, index } => {
            format!("{}[{}]", python_expr(out, of), python_expr(out, index))
        }
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| python_expr(out, a)).collect();
            format!("{}({})", python_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            python_expr(out, left),
            op.python(),
            python_expr(out, right)
        ),
        Expr::Unary { op, operand } => match op {
            UnaryOp::Not => format!("not {}", python_expr(out, operand)),
            UnaryOp::Neg => format!("-{}", python_expr(out, operand)),
        },
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| python_expr(out, i)).collect();
            format!("[{}]", rendered.join(", "))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            // No inline comment: in Python a `#` runs to end of line, so a note inside
            // a call's parentheses swallows the closing paren and the file will not
            // parse. The note lives in the fidelity report, where it can be read.
            "None".to_string()
        }
    }
}

// --------------------------------------------------------------------------- Go

fn go(out: &mut Out, module: &Module) {
    out.line("package main");
    out.blank();
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                let name = go_name(&c.name, c.exported);
                for line in &c.doc {
                    out.line(&format!("// {name} {line}"));
                }
                let value = go_expr(out, &c.value);
                out.line(&format!("const {name} = {value}"));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                let name = go_name(&r.name, true);
                for line in &r.doc {
                    out.line(&format!("// {name} {line}"));
                }
                out.line(&format!("type {name} struct {{"));
                out.open();
                for f in &r.fields {
                    let ty =
                        f.ty.as_ref()
                            .map(go_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    out.line(&format!("{} {ty}", go_name(&f.name, f.exported)));
                }
                out.close();
                out.line("}");
                out.fidelity.records += 1;
                out.blank();
                for m in &r.methods {
                    go_function(out, m, Some(&name));
                    out.blank();
                }
            }
            Item::Function(f) => {
                go_function(out, f, None);
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

/// Go says "exported" with a capital letter, which makes the convention the ACL.
fn go_name(name: &str, exported: bool) -> String {
    if exported {
        pascal(name)
    } else {
        camel(name)
    }
}

fn go_function(out: &mut Out, f: &Function, receiver: Option<&str>) {
    let name = go_name(&f.name, f.exported);
    for line in &f.doc {
        out.line(&format!("// {name} {line}"));
    }
    if f.is_async {
        out.line(
            &out.comment(
                "declared async in the source; Go has no async — call this from a goroutine",
            ),
        );
    }
    let mut foreign = false;
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| {
            let ty = match &p.ty {
                Some(t) => {
                    if is_foreign(t) {
                        foreign = true;
                    }
                    go_type(t)
                }
                None => {
                    foreign = true;
                    unknown(out, &p.name)
                }
            };
            format!("{} {ty}", camel(&p.name))
        })
        .collect();
    let returns = match &f.returns {
        Some(Type::Unit) | None => String::new(),
        Some(t) => {
            if is_foreign(t) {
                foreign = true;
            }
            format!(" {}", go_type(t))
        }
    };
    let receiver = receiver.map(|r| format!("(s *{r}) ")).unwrap_or_default();
    out.line(&format!(
        "func {receiver}{name}({}){returns} {{",
        params.join(", ")
    ));
    out.open();
    go_block(out, &f.body, f.returns.as_ref());
    out.close();
    out.line("}");

    out.fidelity.functions += 1;
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else {
        out.fidelity.signatures_complete += 1;
    }
}

fn go_block(out: &mut Out, body: &[Stmt], returns: Option<&Type>) {
    if body.is_empty() {
        out.line(&out.comment(&format!("{MARKER}: the source had no body to translate")));
        if let Some(t) = returns {
            let zero = go_zero(t);
            out.line(&format!("return {zero}"));
        }
        return;
    }
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", go_expr(out, v)))
                    .unwrap_or_default();
                out.line(&format!("return{text}"));
            }
            Stmt::Let { name, value, .. } => {
                let v = value
                    .as_ref()
                    .map(|v| go_expr(out, v))
                    .unwrap_or_else(|| "nil".to_string());
                out.line(&format!("{} := {v}", camel(name)));
            }
            Stmt::Assign { target, value } => {
                let t = go_expr(out, target);
                let v = go_expr(out, value);
                out.line(&format!("{t} = {v}"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = go_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                go_block(out, then, None);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    go_block(out, otherwise, None);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::While { condition, body } => {
                // Go spells `while` as a one-clause `for`.
                let c = go_expr(out, condition);
                out.line(&format!("for {c} {{"));
                out.open();
                go_block(out, body, None);
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = go_expr(out, iterable);
                out.line(&format!("for _, {} := range {it} {{", camel(binding)));
                out.open();
                go_block(out, body, None);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = go_expr(out, e);
                out.line(&text);
            }
            Stmt::Break => out.line("break"),
            Stmt::Continue => out.line("continue"),
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
}

fn go_type(ty: &Type) -> String {
    match ty {
        // Go writes "returns nothing" by writing nothing, which the return position
        // handles. Everywhere else — a field, a generic argument — it needs a type,
        // and `Result[, string]` is not one.
        Type::Unit => "struct{}".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float64".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("[]{}", go_type(inner)),
        Type::Map(k, v) => format!("map[{}]{}", go_type(k), go_type(v)),
        Type::Optional(inner) => format!("*{}", go_type(inner)),
        Type::Named { name, args } => generic(name, args, "[", "]", ".", go_type),
    }
}

/// Go has no `undefined`; a function that must return something returns its zero.
fn go_zero(ty: &Type) -> String {
    match ty {
        Type::Bool => "false".to_string(),
        Type::Int => "0".to_string(),
        Type::Float => "0".to_string(),
        Type::String => "\"\"".to_string(),
        Type::List(_) | Type::Map(_, _) | Type::Optional(_) => "nil".to_string(),
        Type::Unit => String::new(),
        Type::Named { name, .. } => format!("{}{{}}", generic(name, &[], "[", "]", ".", go_type)),
    }
}

fn go_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "nil".to_string(),
        Expr::Name(n) => camel(n),
        // Not re-cased: `reading.get(…)` names a real method on a value whose type
        // this does not know, and `reading.Get(…)` is a different method.
        Expr::Field { of, name } => format!("{}.{}", go_expr(out, of), name),
        Expr::Index { of, index } => format!("{}[{}]", go_expr(out, of), go_expr(out, index)),
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            format!("{}({})", go_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            go_expr(out, left),
            op.c_like(),
            go_expr(out, right)
        ),
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", go_expr(out, operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| go_expr(out, i)).collect();
            format!("[]any{{{}}}", rendered.join(", "))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("nil /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

// ------------------------------------------------------------------- TypeScript

fn typescript(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/** {line} */"));
                }
                let annotation =
                    c.ty.as_ref()
                        .map(|t| format!(": {}", ts_type(t)))
                        .unwrap_or_default();
                let value = ts_expr(out, &c.value);
                let export = if c.exported { "export " } else { "" };
                out.line(&format!(
                    "{export}const {}{annotation} = {value};",
                    c.name.to_uppercase()
                ));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                for line in &r.doc {
                    out.line(&format!("/** {line} */"));
                }
                let export = if r.exported { "export " } else { "" };
                // A record with no methods is an interface: it is data, and an
                // interface is what TypeScript calls that.
                if r.methods.is_empty() {
                    out.line(&format!("{export}interface {} {{", pascal(&r.name)));
                    out.open();
                    for f in &r.fields {
                        for line in &f.doc {
                            out.line(&format!("/** {line} */"));
                        }
                        let ty =
                            f.ty.as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        out.line(&format!("{}: {ty};", camel(&f.name)));
                    }
                    out.close();
                    out.line("}");
                } else {
                    out.line(&format!("{export}class {} {{", pascal(&r.name)));
                    out.open();
                    for f in &r.fields {
                        let ty =
                            f.ty.as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        out.line(&format!("{}: {ty};", camel(&f.name)));
                    }
                    for m in &r.methods {
                        out.blank();
                        ts_function(out, m, true);
                    }
                    out.close();
                    out.line("}");
                }
                out.fidelity.records += 1;
                out.blank();
            }
            Item::Function(f) => {
                ts_function(out, f, false);
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn ts_function(out: &mut Out, f: &Function, method: bool) {
    for line in &f.doc {
        out.line(&format!("/** {line} */"));
    }
    let mut foreign = false;
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| {
            let annotation = match &p.ty {
                Some(t) => {
                    if is_foreign(t) {
                        foreign = true;
                    }
                    format!(": {}", ts_type(t))
                }
                None => {
                    foreign = true;
                    ": unknown".to_string()
                }
            };
            let default = p
                .default
                .as_ref()
                .map(|d| format!(" = {}", ts_expr(out, d)))
                .unwrap_or_default();
            format!("{}{annotation}{default}", camel(&p.name))
        })
        .collect();
    let returns = match &f.returns {
        None => String::new(),
        Some(Type::Unit) => ": void".to_string(),
        Some(t) => {
            if is_foreign(t) {
                foreign = true;
            }
            format!(": {}", ts_type(t))
        }
    };
    let prefix = if method {
        String::new()
    } else if f.exported {
        "export function ".to_string()
    } else {
        "function ".to_string()
    };
    let asynchrony = if f.is_async { "async " } else { "" };
    out.line(&format!(
        "{asynchrony}{prefix}{}({}){returns} {{",
        camel(&f.name),
        params.join(", ")
    ));
    out.open();
    ts_block(out, &f.body);
    out.close();
    out.line("}");

    out.fidelity.functions += 1;
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else {
        out.fidelity.signatures_complete += 1;
    }
}

fn ts_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line(&format!("throw new Error(\"{MARKER}\");"));
        return;
    }
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", ts_expr(out, v)))
                    .unwrap_or_default();
                out.line(&format!("return{text};"));
            }
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| format!(": {}", ts_type(t)))
                    .unwrap_or_default();
                let keyword = if *mutable { "let" } else { "const" };
                let v = value
                    .as_ref()
                    .map(|v| ts_expr(out, v))
                    .unwrap_or_else(|| "undefined".to_string());
                out.line(&format!("{keyword} {}{annotation} = {v};", camel(name)));
            }
            Stmt::Assign { target, value } => {
                let t = ts_expr(out, target);
                let v = ts_expr(out, value);
                out.line(&format!("{t} = {v};"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = ts_expr(out, condition);
                out.line(&format!("if ({c}) {{"));
                out.open();
                ts_block(out, then);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    ts_block(out, otherwise);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::While { condition, body } => {
                let c = ts_expr(out, condition);
                out.line(&format!("while ({c}) {{"));
                out.open();
                ts_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = ts_expr(out, iterable);
                out.line(&format!("for (const {} of {it}) {{", camel(binding)));
                out.open();
                ts_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = ts_expr(out, e);
                out.line(&format!("{text};"));
            }
            Stmt::Break => out.line("break;"),
            Stmt::Continue => out.line("continue;"),
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
}

fn ts_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        Type::Bool => "boolean".to_string(),
        // TypeScript has one numeric type, and pretending otherwise would be a lie
        // about the signature.
        Type::Int | Type::Float => "number".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("{}[]", ts_type(inner)),
        Type::Map(k, v) => format!("Record<{}, {}>", ts_type(k), ts_type(v)),
        Type::Optional(inner) => format!("{} | null", ts_type(inner)),
        Type::Named { name, args } => generic(name, args, "<", ">", ".", ts_type),
    }
}

fn ts_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "null".to_string(),
        Expr::Name(n) => camel(n),
        Expr::Field { of, name } => format!("{}.{}", ts_expr(out, of), name),
        Expr::Index { of, index } => format!("{}[{}]", ts_expr(out, of), ts_expr(out, index)),
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| ts_expr(out, a)).collect();
            format!("{}({})", ts_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => {
            let spelling = match op {
                BinaryOp::Eq => "===",
                BinaryOp::Ne => "!==",
                other => other.c_like(),
            };
            format!("{} {spelling} {}", ts_expr(out, left), ts_expr(out, right))
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", ts_expr(out, operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| ts_expr(out, i)).collect();
            format!("[{}]", rendered.join(", "))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("null /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

// ------------------------------------------------------------------------ shared

/// A named type, spelled the way this target spells generics.
///
/// The name is deliberately *not* case-converted: a `Reading` or an `HttpResponse` is
/// a real type somewhere, and renaming it to suit a convention would point the
/// signature at something that does not exist.
///
/// A name that cannot be written as a type at all — a tuple, a closure, a trait
/// object — becomes the target's unknown type. Emitting it verbatim produced
/// `-> Result<(), String>` in a Python file, which Python cannot parse; and a
/// signature that does not parse is worse than one that admits a gap.
/// A qualified name arrives spelled the source language's way: Go's `sync.Mutex` is
/// not Rust, and Rust's `std::sync::Mutex` is not Go. The path is kept — it says where
/// the type came from — and only `separator` changes.
fn generic(
    name: &str,
    args: &[Type],
    open: &str,
    close: &str,
    separator: &str,
    render: fn(&Type) -> String,
) -> String {
    if !Type::is_writable_name(name) {
        return format!("Unwritable_{}", sanitise(name));
    }
    let clean = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .split("::")
        .flat_map(|part| part.split('.'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(separator);
    if args.is_empty() {
        return clean;
    }
    let rendered: Vec<String> = args.iter().map(render).collect();
    format!("{clean}{open}{}{close}", rendered.join(", "))
}

/// A name that is not a type, turned into something that at least parses.
fn sanitise(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Is this a type the IR only knows the name of?
///
/// A signature containing one is carried across but is not a *complete* translation,
/// and the report says so rather than letting a renamed unknown look like a promise.
fn is_foreign(ty: &Type) -> bool {
    match ty {
        Type::Named { .. } => true,
        Type::List(inner) | Type::Optional(inner) => is_foreign(inner),
        Type::Map(k, v) => is_foreign(k) || is_foreign(v),
        _ => false,
    }
}

/// A type the source never wrote down. Named rather than guessed.
fn unknown(out: &mut Out, of: &str) -> String {
    out.fidelity
        .notes
        .push(format!("`{of}` had no declared type in the source"));
    match out.language {
        Language::Rust => "()".to_string(),
        Language::Python => "object".to_string(),
        Language::Go => "any".to_string(),
        _ => "unknown".to_string(),
    }
}
