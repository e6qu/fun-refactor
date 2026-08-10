//! Go template actions in Helm charts, parsed and not pattern-matched.
//!
//! `src/parse.rs` masks every `{{ ... }}` action to spaces before handing the file
//! to the YAML grammar, keeping the YAML tree well-formed and every byte offset
//! indexing the original source — at the cost of hiding everything inside an action
//! from the YAML queries. This module reads the spans `Parsed::masked_spans`
//! records and turns each into a structured [`Action`], so `.Values` references,
//! control flow and named templates are parsed, not pattern-matched.
//!
//! What it models, following `text/template`:
//!
//! - **Pipelines and arguments**: `{{ .Values.x | default "y" | quote }}` and
//!   `{{ include "c.name" . }}` both name their operands; the lexer sees tokens, so
//!   a `.Values` path inside a function argument reads the same as a bare one.
//! - **Control actions**: `if`/`else if`/`else`/`end`, `range`, `with`, `define`,
//!   `block`, `template`. Each opener pairs with its `end` into a [`Region`], which
//!   expresses "this key exists only when `.Values.resources` is set" instead of
//!   marking the whole file render-dependent.
//! - **Trim markers**: `{{- ` and ` -}}`, using Go's own rule that the hyphen counts
//!   only when a space character sits between it and the content.
//! - **Built-in objects**: `.Release`, `.Chart`, `.Capabilities`, `.Files`,
//!   `.Template` and `.Subcharts`, kept apart from `.Values` — `.Release.Name` names
//!   no key a values file can hold.
//!
//! What it does not model, it reports. `.field` under a `range` is a field of the
//! element, and the dot inside a `define` is whatever the caller passed: both resolve
//! to [`RefRoot::Context`] with no values path. A `with` rebinds the dot to exactly
//! one value, which does resolve ([`Template::values_path_of`]).
//!
//! `index .Values "a-b"` resolves, since that is how a chart reaches a key whose name
//! is not an identifier. Only literal string arguments resolve; a computed key
//! (`index .Values $k`) or a nested call reports through [`Action::problems`].
//!
//! # The command line
//!
//! `-f` files and `--set` assignments outrank everything in the chart, and a workspace
//! scan sees neither. [`SetValue`] parses Helm's `--set` syntax so a caller that knows
//! the invocation can supply it;
//! [`crate::analysis::provenance::ValuesInputs`] applies it in precedence order.

use crate::parse::Parsed;
use crate::span::Span;
use anyhow::{bail, Result};

/// Helm's built-in top-level objects. None of them lives in a values file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Release,
    Chart,
    Capabilities,
    Files,
    Template,
    Subcharts,
}

impl Builtin {
    pub fn from_segment(segment: &str) -> Option<Self> {
        Some(match segment {
            "Release" => Builtin::Release,
            "Chart" => Builtin::Chart,
            "Capabilities" => Builtin::Capabilities,
            "Files" => Builtin::Files,
            "Template" => Builtin::Template,
            "Subcharts" => Builtin::Subcharts,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Builtin::Release => "Release",
            Builtin::Chart => "Chart",
            Builtin::Capabilities => "Capabilities",
            Builtin::Files => "Files",
            Builtin::Template => "Template",
            Builtin::Subcharts => "Subcharts",
        }
    }
}

/// What a field chain is rooted at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefRoot {
    /// `.Values.a.b` — the chart's values, which a values file can supply.
    Values,
    /// `.Release.Name` and friends, supplied by Helm at render time.
    Builtin(Builtin),
    /// A field of the current dot: `.name` inside a `range`, `with` or `define`.
    Context,
    /// `$name.a.b`. The name is empty for `$` itself.
    Variable(String),
    /// The bare dot, as in `{{ include "c.labels" . }}`.
    Dot,
}

/// One field chain named by an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub root: RefRoot,
    /// The segments after the root: `.Values.image.tag` gives `["image", "tag"]`.
    pub path: Vec<String>,
    /// Byte span of the chain in the file the action came from.
    pub span: Span,
    /// The chain exactly as written.
    pub text: String,
}

impl Ref {
    /// The values path this names, for a chain rooted at `.Values`.
    pub fn values_path(&self) -> Option<&[String]> {
        matches!(self.root, RefRoot::Values).then_some(self.path.as_slice())
    }

    pub fn builtin(&self) -> Option<Builtin> {
        match self.root {
            RefRoot::Builtin(builtin) => Some(builtin),
            _ => None,
        }
    }
}

/// What an action does. Control actions are separated from value actions because
/// only the former open or close a [`Region`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    If {
        expression: String,
    },
    ElseIf {
        expression: String,
    },
    Else,
    End,
    Range {
        expression: String,
        variables: Vec<String>,
    },
    With {
        expression: String,
    },
    Define {
        name: String,
    },
    /// `{{ block "name" . }}` both defines a template and invokes it here.
    Block {
        name: String,
    },
    /// `{{ template "name" . }}`.
    TemplateCall {
        name: String,
    },
    /// `{{ $x := ... }}` or `{{ $x = ... }}`.
    Assignment {
        variable: String,
    },
    /// `{{/* ... */}}`.
    Comment,
    /// A pipeline whose result is rendered.
    Expression,
    /// `{{ }}`.
    Empty,
}

impl ActionKind {
    /// Does this action open a region that a later `end` closes?
    pub fn opens_region(&self) -> Option<RegionKind> {
        Some(match self {
            ActionKind::If { .. } => RegionKind::If,
            ActionKind::Range { .. } => RegionKind::Range,
            ActionKind::With { .. } => RegionKind::With,
            ActionKind::Define { .. } => RegionKind::Define,
            ActionKind::Block { .. } => RegionKind::Block,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ActionKind::If { .. } => "if",
            ActionKind::ElseIf { .. } => "else if",
            ActionKind::Else => "else",
            ActionKind::End => "end",
            ActionKind::Range { .. } => "range",
            ActionKind::With { .. } => "with",
            ActionKind::Define { .. } => "define",
            ActionKind::Block { .. } => "block",
            ActionKind::TemplateCall { .. } => "template",
            ActionKind::Assignment { .. } => "assignment",
            ActionKind::Comment => "comment",
            ActionKind::Expression => "expression",
            ActionKind::Empty => "empty",
        }
    }
}

/// One `{{ ... }}` action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Byte span of the whole action, delimiters included, in the original file.
    pub span: Span,
    /// The action exactly as written.
    pub text: String,
    pub kind: ActionKind,
    /// `{{-` was written.
    pub trim_left: bool,
    /// `-}}` was written.
    pub trim_right: bool,
    /// Every field chain the action names, in source order.
    pub refs: Vec<Ref>,
    /// Function names the action calls, in source order.
    pub functions: Vec<String>,
    /// Named templates the action invokes, through `include`, `template` or `block`.
    pub invokes: Vec<String>,
    /// Index into [`Template::regions`] of the innermost region containing this
    /// action. An opener is *not* inside the region it opens.
    pub enclosing: Option<usize>,
    /// Anything the lexer could not account for, kept and not dropped.
    pub problems: Vec<String>,
}

impl Action {
    /// Every `.Values` path named directly by this action, ignoring any enclosing
    /// `with`. Use [`Template::values_paths_of`] when the context matters.
    pub fn values_paths(&self) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = Vec::new();
        for path in self.refs.iter().filter_map(Ref::values_path) {
            if !path.is_empty() && !out.iter().any(|seen| seen == path) {
                out.push(path.to_vec());
            }
        }
        out
    }

    /// The built-in objects this action reads, in source order, without repeats.
    pub fn builtins(&self) -> Vec<Builtin> {
        let mut out: Vec<Builtin> = Vec::new();
        for builtin in self.refs.iter().filter_map(Ref::builtin) {
            if !out.contains(&builtin) {
                out.push(builtin);
            }
        }
        out
    }

    /// The expression a control action tests or iterates, as written.
    pub fn expression(&self) -> Option<&str> {
        match &self.kind {
            ActionKind::If { expression }
            | ActionKind::ElseIf { expression }
            | ActionKind::Range { expression, .. }
            | ActionKind::With { expression } => Some(expression),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    If,
    Range,
    With,
    Define,
    Block,
}

impl RegionKind {
    /// Does the region's body render only under some condition?
    ///
    /// `with` counts: it skips its body when the value is empty. `range` counts: a
    /// zero-length collection renders nothing. `define` does not — its body renders
    /// wherever it is included, which is a different question.
    pub fn is_conditional(&self) -> bool {
        matches!(self, RegionKind::If | RegionKind::Range | RegionKind::With)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RegionKind::If => "if",
            RegionKind::Range => "range",
            RegionKind::With => "with",
            RegionKind::Define => "define",
            RegionKind::Block => "block",
        }
    }
}

/// An opener and its `end`, with everything between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub kind: RegionKind,
    /// The tested expression, or the quoted name for `define`/`block`.
    pub subject: String,
    /// Index into [`Template::actions`] of the opening action.
    pub open: usize,
    /// Index of the matching `end`, absent when the file never closes the region.
    pub end: Option<usize>,
    /// Indices of the `else` and `else if` actions belonging to this region.
    pub branches: Vec<usize>,
    /// Opener through `end`, delimiters included.
    pub span: Span,
    /// Between the opener and the `end`: the bytes the region governs.
    pub body: Span,
    pub parent: Option<usize>,
    pub depth: usize,
}

/// Which arm of a region a byte offset falls in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Branch {
    /// Before any `else`.
    Then,
    ElseIf(String),
    Else,
}

/// A region enclosing some point, with the arm that point sits in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    pub kind: RegionKind,
    pub subject: String,
    pub branch: Branch,
    /// Index into [`Template::regions`].
    pub region: usize,
    /// The opening action's span.
    pub span: Span,
}

impl Guard {
    pub fn is_conditional(&self) -> bool {
        self.kind.is_conditional()
    }

    /// The condition in the words the chart author wrote.
    pub fn describe(&self) -> String {
        match (&self.branch, self.kind) {
            (Branch::Then, RegionKind::Define | RegionKind::Block) => {
                format!("{} {}", self.kind.as_str(), self.subject)
            }
            (Branch::Then, _) => format!("{} {}", self.kind.as_str(), self.subject),
            (Branch::ElseIf(condition), _) => format!(
                "else if {condition} (of {} {})",
                self.kind.as_str(),
                self.subject
            ),
            (Branch::Else, _) => {
                format!("the else branch of {} {}", self.kind.as_str(), self.subject)
            }
        }
    }
}

/// A `{{ define "name" }}` … `{{ end }}` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedTemplate {
    pub name: String,
    /// Index into [`Template::regions`].
    pub region: usize,
    /// The body between `define` and `end`.
    pub body: Span,
    /// The whole block.
    pub span: Span,
}

/// A call of a named template: `include "name" .`, `template "name" .`, or the
/// implicit call a `block` makes at its own site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub name: String,
    /// Index into [`Template::actions`].
    pub action: usize,
    pub span: Span,
}

/// Every template action in one file, with its nesting resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub actions: Vec<Action>,
    pub regions: Vec<Region>,
    pub defines: Vec<NamedTemplate>,
    pub invocations: Vec<Invocation>,
    /// Openers with no `end`, and `end`s (or `else`s) closing nothing. A Go
    /// template with any of these does not render at all, so they are reported
    /// and not repaired.
    pub unbalanced: Vec<(Span, String)>,
}

/// Parse the actions of one file, given the spans `src/parse.rs` recorded.
pub fn parse(source: &str, actions: &[Span]) -> Template {
    Template::new(source, actions)
}

impl Template {
    /// Parse straight from a [`Parsed`] Helm file.
    pub fn of(source: &str, parsed: &Parsed) -> Template {
        Template::new(source, &parsed.masked_spans)
    }

    fn new(source: &str, spans: &[Span]) -> Template {
        let mut actions: Vec<Action> = spans.iter().map(|span| action_at(source, *span)).collect();
        actions.sort_by_key(|a| a.span.start);

        let mut regions: Vec<Region> = Vec::new();
        let mut unbalanced: Vec<(Span, String)> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();

        for index in 0..actions.len() {
            let span = actions[index].span;
            match actions[index].kind.clone() {
                ActionKind::End => {
                    match stack.pop() {
                        Some(region) => {
                            regions[region].end = Some(index);
                            regions[region].span = Span::new(regions[region].span.start, span.end);
                            regions[region].body =
                                Span::new(actions[regions[region].open].span.end, span.start);
                        }
                        None => unbalanced.push((span, "`end` closes nothing".to_string())),
                    }
                    actions[index].enclosing = stack.last().copied();
                }
                ActionKind::Else | ActionKind::ElseIf { .. } => {
                    match stack.last() {
                        Some(region) => regions[*region].branches.push(index),
                        None => unbalanced.push((
                            span,
                            format!("`{}` belongs to no if", actions[index].kind.as_str()),
                        )),
                    }
                    actions[index].enclosing = stack.last().copied();
                }
                kind => {
                    actions[index].enclosing = stack.last().copied();
                    if let Some(region_kind) = kind.opens_region() {
                        let subject = match &kind {
                            ActionKind::If { expression }
                            | ActionKind::Range { expression, .. }
                            | ActionKind::With { expression } => expression.clone(),
                            ActionKind::Define { name } | ActionKind::Block { name } => {
                                format!("{name:?}")
                            }
                            _ => String::new(),
                        };
                        regions.push(Region {
                            kind: region_kind,
                            subject,
                            open: index,
                            end: None,
                            branches: Vec::new(),
                            span,
                            body: Span::new(span.end, source.len()),
                            parent: stack.last().copied(),
                            depth: stack.len(),
                        });
                        stack.push(regions.len() - 1);
                    }
                }
            }
        }
        for region in stack {
            unbalanced.push((
                regions[region].span,
                format!(
                    "`{}` is never closed by an `end`",
                    regions[region].kind.as_str()
                ),
            ));
        }

        let defines = regions
            .iter()
            .enumerate()
            .filter_map(|(index, region)| {
                let name = match &actions[region.open].kind {
                    ActionKind::Define { name } | ActionKind::Block { name } => name.clone(),
                    _ => return None,
                };
                Some(NamedTemplate {
                    name,
                    region: index,
                    body: region.body,
                    span: region.span,
                })
            })
            .collect();

        let invocations = actions
            .iter()
            .enumerate()
            .flat_map(|(index, action)| {
                action.invokes.iter().map(move |name| Invocation {
                    name: name.clone(),
                    action: index,
                    span: action.span,
                })
            })
            .collect();

        Template {
            actions,
            regions,
            defines,
            invocations,
            unbalanced,
        }
    }

    /// Every `.Values` path any action in the file names, without repeats.
    pub fn values_paths(&self) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = Vec::new();
        for index in 0..self.actions.len() {
            for path in self.values_paths_of(index) {
                if !out.contains(&path) {
                    out.push(path);
                }
            }
        }
        out
    }

    /// The `.Values` paths one action names, resolving `.field` against an
    /// enclosing `with`. Fields whose dot is bound by a `range` or a `define`
    /// yield nothing: no caller knows what the dot holds there.
    pub fn values_paths_of(&self, action: usize) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = Vec::new();
        for reference in &self.actions[action].refs {
            let Some(path) = self.values_path_of(action, reference) else {
                continue;
            };
            if !path.is_empty() && !out.contains(&path) {
                out.push(path);
            }
        }
        out
    }

    /// The values path one reference names in the context of its action.
    pub fn values_path_of(&self, action: usize, reference: &Ref) -> Option<Vec<String>> {
        match &reference.root {
            RefRoot::Values => Some(reference.path.clone()),
            RefRoot::Context => {
                let mut path = self.with_prefix(self.actions[action].enclosing)?;
                path.extend(reference.path.iter().cloned());
                Some(path)
            }
            _ => None,
        }
    }

    /// The values subtree an enclosing `with` bound the dot to, if it bound one.
    fn with_prefix(&self, region: Option<usize>) -> Option<Vec<String>> {
        let mut current = region;
        while let Some(index) = current {
            let region = &self.regions[index];
            match region.kind {
                // Only `with` rebinds the dot to exactly one value.
                RegionKind::With => {
                    let opener = &self.actions[region.open];
                    let reference = opener.refs.first()?;
                    return match &reference.root {
                        RefRoot::Values => Some(reference.path.clone()),
                        RefRoot::Context => {
                            let mut path = self.with_prefix(region.parent)?;
                            path.extend(reference.path.iter().cloned());
                            Some(path)
                        }
                        _ => None,
                    };
                }
                // The dot is an element, or whatever the caller passed.
                RegionKind::Range | RegionKind::Define | RegionKind::Block => return None,
                // An `if` leaves the dot alone.
                RegionKind::If => current = region.parent,
            }
        }
        None
    }

    /// The action containing a byte offset.
    pub fn action_at(&self, offset: usize) -> Option<(usize, &Action)> {
        self.actions
            .iter()
            .enumerate()
            .find(|(_, action)| action.span.contains_offset(offset))
    }

    /// Actions whose opening delimiter falls inside `span`, in source order.
    pub fn actions_in(&self, span: Span) -> Vec<(usize, &Action)> {
        self.actions
            .iter()
            .enumerate()
            .filter(|(_, action)| span.contains_offset(action.span.start))
            .collect()
    }

    /// Every region governing a byte offset, outermost first.
    pub fn guards_at(&self, offset: usize) -> Vec<Guard> {
        let innermost = self
            .regions
            .iter()
            .enumerate()
            .filter(|(_, region)| region.body.contains_offset(offset))
            .min_by_key(|(_, region)| region.body.len())
            .map(|(index, _)| index);

        let mut chain = Vec::new();
        let mut current = innermost;
        while let Some(index) = current {
            chain.push(index);
            current = self.regions[index].parent;
        }
        chain.reverse();
        chain
            .into_iter()
            .map(|index| self.guard(index, offset))
            .collect()
    }

    /// Only the regions that decide whether the bytes at `offset` render at all.
    pub fn conditions_at(&self, offset: usize) -> Vec<Guard> {
        self.guards_at(offset)
            .into_iter()
            .filter(Guard::is_conditional)
            .collect()
    }

    fn guard(&self, region: usize, offset: usize) -> Guard {
        let branch = self.regions[region]
            .branches
            .iter()
            .filter(|index| self.actions[**index].span.end <= offset)
            .max()
            .map(|index| match &self.actions[*index].kind {
                ActionKind::ElseIf { expression } => Branch::ElseIf(expression.clone()),
                _ => Branch::Else,
            })
            .unwrap_or(Branch::Then);
        Guard {
            kind: self.regions[region].kind,
            subject: self.regions[region].subject.clone(),
            branch,
            region,
            span: self.actions[self.regions[region].open].span,
        }
    }

    /// The innermost `define`/`block` whose body holds `offset`.
    pub fn define_containing(&self, offset: usize) -> Option<&NamedTemplate> {
        self.defines
            .iter()
            .filter(|define| define.body.contains_offset(offset))
            .min_by_key(|define| define.body.len())
    }

    pub fn define(&self, name: &str) -> Option<&NamedTemplate> {
        self.defines.iter().find(|define| define.name == name)
    }

    /// Every call of one named template in this file.
    pub fn invocations_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Invocation> {
        self.invocations
            .iter()
            .filter(move |invocation| invocation.name == name)
    }
}

/// Parse a single action from its text alone.
///
/// The delimiters are optional: text with none is read as the inside of an action,
/// which is what makes this usable on a fragment. Spans are relative to `text`.
pub fn parse_action(text: &str) -> Action {
    action_at(text, Span::new(0, text.len()))
}

/// Every `.Values.a.b.c` path named in a snippet of template text.
///
/// Context-relative fields are not included: without the file around them there is
/// no `with` to resolve them against.
pub fn values_paths_in(text: &str) -> Vec<Vec<String>> {
    parse_action(text).values_paths()
}

/// Helm's built-in objects named in a snippet, as `.Release`-style names.
pub fn builtins_in(text: &str) -> Vec<String> {
    parse_action(text)
        .builtins()
        .into_iter()
        .map(|builtin| format!(".{}", builtin.as_str()))
        .collect()
}

// ------------------------------------------------------- `--set` on the CLI

/// One step of a `--set` path: `image.tag` is two keys, `ports[0].name` is a key,
/// an index and a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetSegment {
    Key(String),
    Index(usize),
}

impl std::fmt::Display for SetSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetSegment::Key(name) => write!(f, "{name}"),
            SetSegment::Index(i) => write!(f, "[{i}]"),
        }
    }
}

/// One `--set` or `--set-string` assignment from a `helm` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetValue {
    /// The path as written, indices included.
    pub path: Vec<SetSegment>,
    /// The value, with `\.`, `\,`, `\=` and `\\` escapes resolved.
    pub value: String,
    /// `--set-string`: Helm keeps the value a string instead of coercing it.
    pub string: bool,
    /// The assignment exactly as written, e.g. `image.tag=1.2`.
    pub text: String,
}

impl SetValue {
    /// The mapping keys of the path, with list indices dropped.
    ///
    /// Values-file keys are indexed by their mapping path — a key under a sequence
    /// is qualified by the sequence's key, with no index — so `ports[0].name` and
    /// the `name` under `ports:` are the same key path here.
    pub fn keys(&self) -> Vec<String> {
        self.path
            .iter()
            .filter_map(|segment| match segment {
                SetSegment::Key(name) => Some(name.clone()),
                SetSegment::Index(_) => None,
            })
            .collect()
    }

    pub fn flag(&self) -> &'static str {
        if self.string {
            "--set-string"
        } else {
            "--set"
        }
    }

    /// The assignment as it would be written on the command line.
    pub fn describe(&self) -> String {
        format!("`{} {}`", self.flag(), self.text)
    }
}

impl std::fmt::Display for SetValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.flag(), self.text)
    }
}

/// Parse one `--set`/`--set-string` argument, which may hold several assignments
/// separated by unescaped commas, as Helm's `strvals` does.
///
/// What is not supported is refused by name and not half-applied: the `{a,b}`
/// list literal has no single key to compete for, so it is rejected with the
/// alternative that does work.
pub fn parse_set(argument: &str, string: bool) -> Result<Vec<SetValue>> {
    if argument.trim().is_empty() {
        bail!("`--set` was given nothing to set; it takes key=value");
    }
    if unescaped_positions(argument, '{').next().is_some() {
        bail!(
            "`{argument}` uses Helm's `{{a,b}}` list syntax, which names no single key to \
             rank; pass the list in a `-f` values file instead"
        );
    }

    let mut out = Vec::new();
    for assignment in split_unescaped(argument, ',') {
        let assignment = assignment.trim().to_string();
        if assignment.is_empty() {
            bail!("`{argument}` holds an empty assignment; --set takes key=value[,key=value]");
        }
        let Some((key, value)) = split_once_unescaped(&assignment, '=') else {
            bail!("`{assignment}` is not an assignment; --set takes key=value");
        };
        let path = parse_set_path(&key)?;
        out.push(SetValue {
            path,
            value: unescape(&value),
            string,
            text: assignment,
        });
    }
    Ok(out)
}

/// `image.tag`, `ports[0].name`, `annotations.foo\.bar` — Helm's key syntax.
fn parse_set_path(key: &str) -> Result<Vec<SetSegment>> {
    if key.trim().is_empty() {
        bail!("`{key}=…` sets an empty key; --set takes key=value");
    }
    let mut segments = Vec::new();
    for part in split_unescaped(key, '.') {
        let bytes = part.as_bytes();
        let name_end = part.find('[').unwrap_or(part.len());
        let name = unescape(&part[..name_end]);
        if !name.is_empty() {
            segments.push(SetSegment::Key(name));
        } else if name_end != 0 {
            bail!("`{key}` has an empty path segment");
        }
        let mut i = name_end;
        while i < bytes.len() {
            if bytes[i] != b'[' {
                bail!(
                    "`{key}` is not a Helm --set path: expected `[` at '{}'",
                    &part[i..]
                );
            }
            let Some(close) = part[i..].find(']').map(|offset| i + offset) else {
                bail!("`{key}` has an unclosed `[`");
            };
            let digits = &part[i + 1..close];
            let index: usize = digits.parse().map_err(|_| {
                anyhow::anyhow!("`{key}` indexes with '{digits}', which is not a list index")
            })?;
            segments.push(SetSegment::Index(index));
            i = close + 1;
        }
    }
    if segments.is_empty() {
        bail!("`{key}` names no key");
    }
    Ok(segments)
}

/// Offsets of `needle` that are not preceded by an odd run of backslashes.
fn unescaped_positions(text: &str, needle: char) -> impl Iterator<Item = usize> + '_ {
    text.char_indices().filter_map(move |(i, c)| {
        if c != needle {
            return None;
        }
        let escapes = text[..i].chars().rev().take_while(|c| *c == '\\').count();
        (escapes % 2 == 0).then_some(i)
    })
}

fn split_unescaped(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    for at in unescaped_positions(text, separator).collect::<Vec<_>>() {
        parts.push(text[start..at].to_string());
        start = at + separator.len_utf8();
    }
    parts.push(text[start..].to_string());
    parts
}

fn split_once_unescaped(text: &str, separator: char) -> Option<(String, String)> {
    let at = unescaped_positions(text, separator).next()?;
    Some((
        text[..at].to_string(),
        text[at + separator.len_utf8()..].to_string(),
    ))
}

/// Resolve Helm's `\.`, `\,`, `\=` and `\\` escapes; anything else keeps its
/// backslash, which is what Helm does with it.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(escaped @ ('.' | ',' | '=' | '\\' | '[' | ']')) => out.push(escaped),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ------------------------------------------------------------------ the lexer

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    /// `.Values.image.tag` — the segments after each dot.
    Field(Vec<String>),
    /// A bare `.`.
    Dot,
    Variable {
        name: String,
        path: Vec<String>,
    },
    Str(String),
    Number(String),
    LParen,
    RParen,
    Pipe,
    Comma,
    /// `:=`
    Declare,
    /// `=`
    Assign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: Tok,
    span: Span,
}

/// Keywords `text/template` reserves in the leading position of an action.
const KEYWORDS: &[&str] = &[
    "if", "else", "end", "range", "with", "define", "block", "template", "break", "continue",
];

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Build one action from its span in `source`.
fn action_at(source: &str, span: Span) -> Action {
    let text = source
        .get(span.start..span.end)
        .unwrap_or_default()
        .to_string();

    let mut inner_start = span.start + if text.starts_with("{{") { 2 } else { 0 };
    let mut inner_end = span.end
        - if text.len() >= inner_start - span.start + 2 && text.ends_with("}}") {
            2
        } else {
            0
        };
    if inner_end < inner_start {
        inner_end = inner_start;
    }

    // Go's rule: the hyphen is a trim marker only with a space beside it, so
    // `{{-3}}` is the number -3 and `{{- 3}}` is a trimmed 3.
    let inner = &source[inner_start..inner_end];
    let trim_left = inner
        .strip_prefix('-')
        .is_some_and(|rest| rest.starts_with(char::is_whitespace));
    if trim_left {
        inner_start += 1;
    }
    let inner = &source[inner_start..inner_end];
    let trim_right = inner
        .strip_suffix('-')
        .is_some_and(|rest| rest.ends_with(char::is_whitespace));
    if trim_right {
        inner_end -= 1;
    }

    let inner = &source[inner_start..inner_end];
    // Text with no delimiters at all is a fragment, which is a supported input; an
    // action that opens and never closes is a broken file, which is not.
    let mut problems = Vec::new();
    if text.starts_with("{{") && !text.ends_with("}}") {
        problems.push("action is never closed by `}}`".to_string());
    }

    if inner.trim_start().starts_with("/*") {
        return Action {
            span,
            text,
            kind: ActionKind::Comment,
            trim_left,
            trim_right,
            refs: Vec::new(),
            functions: Vec::new(),
            invokes: Vec::new(),
            enclosing: None,
            problems,
        };
    }

    let tokens = lex(inner, inner_start, &mut problems);
    let kind = classify(&tokens, source, &mut problems);
    let refs = references(&tokens, source, &mut problems);
    let functions = function_names(&tokens, &kind);
    let invokes = invocations(&tokens, &kind);

    Action {
        span,
        text,
        kind,
        trim_left,
        trim_right,
        refs,
        functions,
        invokes,
        enclosing: None,
        problems,
    }
}

fn lex(inner: &str, base: usize, problems: &mut Vec<String>) -> Vec<Token> {
    let bytes = inner.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = match c {
            b'(' => {
                i += 1;
                Tok::LParen
            }
            b')' => {
                i += 1;
                Tok::RParen
            }
            b'|' => {
                i += 1;
                Tok::Pipe
            }
            b',' => {
                i += 1;
                Tok::Comma
            }
            b'=' => {
                i += 1;
                Tok::Assign
            }
            b':' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 2;
                    Tok::Declare
                } else {
                    problems.push("a bare ':' is not a template token".to_string());
                    i += 1;
                    continue;
                }
            }
            b'"' | b'`' | b'\'' => {
                let (value, next) = lex_string(inner, i, problems);
                i = next;
                Tok::Str(value)
            }
            b'.' => {
                let (segments, next) = lex_segments(inner, i);
                i = next;
                if segments.is_empty() {
                    Tok::Dot
                } else {
                    Tok::Field(segments)
                }
            }
            b'$' => {
                let mut j = i + 1;
                while j < bytes.len() && is_ident_char(bytes[j]) {
                    j += 1;
                }
                let name = inner[i + 1..j].to_string();
                let (path, next) = lex_segments(inner, j);
                i = next;
                Tok::Variable { name, path }
            }
            b'+' | b'-' if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                let (value, next) = lex_number(inner, i);
                i = next;
                Tok::Number(value)
            }
            c if c.is_ascii_digit() => {
                let (value, next) = lex_number(inner, i);
                i = next;
                Tok::Number(value)
            }
            c if is_ident_start(c) => {
                let mut j = i;
                while j < bytes.len() && is_ident_char(bytes[j]) {
                    j += 1;
                }
                let name = inner[i..j].to_string();
                i = j;
                Tok::Ident(name)
            }
            other => {
                problems.push(format!(
                    "unexpected character '{}' in a template action",
                    other as char
                ));
                i += 1;
                continue;
            }
        };
        tokens.push(Token {
            kind,
            span: Span::new(base + start, base + i),
        });
    }
    tokens
}

/// Read `.a.b.c` starting at `at`, returning the segments and the offset after them.
fn lex_segments(inner: &str, at: usize) -> (Vec<String>, usize) {
    let bytes = inner.as_bytes();
    let mut segments = Vec::new();
    let mut i = at;
    while i < bytes.len() && bytes[i] == b'.' {
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && is_ident_char(bytes[j]) {
            j += 1;
        }
        if j == start {
            break;
        }
        segments.push(inner[start..j].to_string());
        i = j;
    }
    if segments.is_empty() && i < bytes.len() && bytes[i] == b'.' {
        // A bare dot: consume it so the caller makes progress.
        i += 1;
    }
    (segments, i)
}

fn lex_string(inner: &str, at: usize, problems: &mut Vec<String>) -> (String, usize) {
    let bytes = inner.as_bytes();
    let quote = bytes[at];
    let mut value = String::new();
    let mut i = at + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && quote != b'`' {
            // Escapes are preserved verbatim: nothing here interprets them, and a
            // decoded value would no longer match the source.
            if let Some(next) = inner.get(i..i + 2) {
                value.push_str(next);
            }
            i += 2;
            continue;
        }
        if c == quote {
            return (value, i + 1);
        }
        let end = inner[i..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| i + offset)
            .unwrap_or(inner.len());
        value.push_str(&inner[i..end]);
        i = end;
    }
    problems.push("a string literal is never closed".to_string());
    (value, inner.len())
}

fn lex_number(inner: &str, at: usize) -> (String, usize) {
    let bytes = inner.as_bytes();
    let mut i = at + 1;
    while i < bytes.len() {
        let c = bytes[i];
        let exponent_sign = matches!(c, b'+' | b'-')
            && matches!(bytes[i - 1], b'e' | b'E')
            && !inner[at..i].starts_with("0x");
        if c.is_ascii_alphanumeric() || c == b'.' || exponent_sign {
            i += 1;
        } else {
            break;
        }
    }
    (inner[at..i].to_string(), i)
}

// ------------------------------------------------------------- classification

/// Source text spanning a run of tokens.
fn token_text(source: &str, tokens: &[Token]) -> String {
    match (tokens.first(), tokens.last()) {
        (Some(first), Some(last)) => source
            .get(first.span.start..last.span.end)
            .unwrap_or_default()
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn classify(tokens: &[Token], source: &str, problems: &mut Vec<String>) -> ActionKind {
    let Some(first) = tokens.first() else {
        return ActionKind::Empty;
    };

    if let Tok::Variable { name, .. } = &first.kind {
        if matches!(
            tokens.get(1).map(|t| &t.kind),
            Some(Tok::Declare) | Some(Tok::Assign)
        ) {
            return ActionKind::Assignment {
                variable: name.clone(),
            };
        }
    }

    let Tok::Ident(word) = &first.kind else {
        return ActionKind::Expression;
    };

    let quoted_name = |from: usize| -> Option<String> {
        match tokens.get(from).map(|t| &t.kind) {
            Some(Tok::Str(name)) => Some(name.clone()),
            _ => None,
        }
    };

    match word.as_str() {
        "if" => ActionKind::If {
            expression: token_text(source, &tokens[1..]),
        },
        "else" => match tokens.get(1).map(|t| &t.kind) {
            Some(Tok::Ident(next)) if next == "if" => ActionKind::ElseIf {
                expression: token_text(source, &tokens[2..]),
            },
            _ => ActionKind::Else,
        },
        "end" => ActionKind::End,
        "with" => ActionKind::With {
            expression: token_text(source, &tokens[1..]),
        },
        "range" => {
            // `range $index, $element := .Values.list` binds before it iterates.
            let declare = tokens.iter().position(|token| token.kind == Tok::Declare);
            let variables = match declare {
                Some(at) => tokens[1..at]
                    .iter()
                    .filter_map(|token| match &token.kind {
                        Tok::Variable { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect(),
                None => Vec::new(),
            };
            let from = declare.map(|at| at + 1).unwrap_or(1);
            ActionKind::Range {
                expression: token_text(source, &tokens[from.min(tokens.len())..]),
                variables,
            }
        }
        "define" | "block" | "template" => {
            let Some(name) = quoted_name(1) else {
                problems.push(format!("`{word}` is not followed by a quoted name"));
                return ActionKind::Expression;
            };
            match word.as_str() {
                "define" => ActionKind::Define { name },
                "block" => ActionKind::Block { name },
                _ => ActionKind::TemplateCall { name },
            }
        }
        _ => ActionKind::Expression,
    }
}

/// Every field chain the tokens name.
fn references(tokens: &[Token], source: &str, problems: &mut Vec<String>) -> Vec<Ref> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        // `index .Values "a-b"` reaches a key no field chain can spell. Its string
        // arguments *are* the path, so it resolves like one; anything else it is
        // given cannot be, and is reported and not dropped.
        if let Some((reference, next)) = index_call(tokens, source, i, problems) {
            if let Some(reference) = reference {
                out.push(reference);
            }
            i = next;
            continue;
        }
        let token = &tokens[i];
        i += 1;
        let text = source
            .get(token.span.start..token.span.end)
            .unwrap_or_default()
            .to_string();
        match &token.kind {
            Tok::Field(segments) => {
                let (root, path) = match Builtin::from_segment(&segments[0]) {
                    Some(builtin) => (RefRoot::Builtin(builtin), segments[1..].to_vec()),
                    None if segments[0] == "Values" => (RefRoot::Values, segments[1..].to_vec()),
                    None => (RefRoot::Context, segments.clone()),
                };
                out.push(Ref {
                    root,
                    path,
                    span: token.span,
                    text,
                });
            }
            // `$` is the root context, so `$.Values.x` names the same key `.Values.x`
            // does — which is how a `range` body reaches past the rebound dot.
            Tok::Variable { name, path } if name.is_empty() && !path.is_empty() => {
                let (root, rest) = match Builtin::from_segment(&path[0]) {
                    Some(builtin) => (RefRoot::Builtin(builtin), path[1..].to_vec()),
                    None if path[0] == "Values" => (RefRoot::Values, path[1..].to_vec()),
                    None => (RefRoot::Variable(String::new()), path.clone()),
                };
                out.push(Ref {
                    root,
                    path: rest,
                    span: token.span,
                    text,
                });
            }
            Tok::Variable { name, path } => out.push(Ref {
                root: RefRoot::Variable(name.clone()),
                path: path.clone(),
                span: token.span,
                text,
            }),
            Tok::Dot => out.push(Ref {
                root: RefRoot::Dot,
                path: Vec::new(),
                span: token.span,
                text,
            }),
            _ => {}
        }
    }
    out
}

/// An `index` call over `.Values`, starting at token `at`.
///
/// Returns the reference it names — `None` inside the `Some` when the call names
/// no key we can know — and the token index to continue from. `None` means the
/// tokens at `at` are not an `index` call at all, and are read the ordinary way.
///
/// `index .Values "a-b" "c"` is `.Values.a-b.c`: each literal string argument is
/// one path segment, which is exactly what Go's `index` does to a map. A computed
/// key or a parenthesised sub-call names a segment the workspace does not hold, so
/// it becomes a problem on the action and not a guessed path.
fn index_call(
    tokens: &[Token],
    source: &str,
    at: usize,
    problems: &mut Vec<String>,
) -> Option<(Option<Ref>, usize)> {
    match &tokens[at].kind {
        Tok::Ident(name) if name == "index" => {}
        _ => return None,
    }

    // The base is the collection being indexed: `.Values`, `.Values.a` or `$.Values`.
    let base = tokens.get(at + 1)?;
    let base_path: Vec<String> = match &base.kind {
        Tok::Field(segments) if segments.first().is_some_and(|s| s == "Values") => {
            segments[1..].to_vec()
        }
        Tok::Variable { name, path }
            if name.is_empty() && path.first().is_some_and(|s| s == "Values") =>
        {
            path[1..].to_vec()
        }
        Tok::LParen => {
            problems.push(
                "`index` is given a parenthesised expression, so the key it reads is not a \
                 values path this can resolve"
                    .to_string(),
            );
            return None;
        }
        _ => return None,
    };

    let mut path = base_path.clone();
    let mut end = at + 2;
    while let Some(Token {
        kind: Tok::Str(segment),
        ..
    }) = tokens.get(end)
    {
        path.push(segment.clone());
        end += 1;
    }

    if end == at + 2 {
        // Nothing literal followed. A chain with a path of its own still names a
        // key — `index .Values.hosts 0` reads an element of `.Values.hosts` — but a
        // bare `.Values` indexed by a computed key names nothing at all.
        if base_path.is_empty() {
            let key = tokens
                .get(at + 2)
                .and_then(|token| source.get(token.span.start..token.span.end))
                .unwrap_or("")
                .trim();
            problems.push(format!(
                "`index .Values {key}` computes its key, so the values path is not resolved"
            ));
        }
        return None;
    }

    let span = Span::new(tokens[at].span.start, tokens[end - 1].span.end);
    Some((
        Some(Ref {
            root: RefRoot::Values,
            path,
            span,
            text: source
                .get(span.start..span.end)
                .unwrap_or_default()
                .to_string(),
        }),
        end,
    ))
}

/// Bare identifiers in a template are function names, bar the keywords and the
/// three constants.
fn function_names(tokens: &[Token], kind: &ActionKind) -> Vec<String> {
    let skip = match kind {
        ActionKind::ElseIf { .. } => 2,
        ActionKind::If { .. }
        | ActionKind::Else
        | ActionKind::End
        | ActionKind::Range { .. }
        | ActionKind::With { .. }
        | ActionKind::Define { .. }
        | ActionKind::Block { .. }
        | ActionKind::TemplateCall { .. } => 1,
        _ => 0,
    };
    tokens
        .iter()
        .skip(skip)
        .filter_map(|token| match &token.kind {
            Tok::Ident(name)
                if !KEYWORDS.contains(&name.as_str())
                    && !matches!(name.as_str(), "true" | "false" | "nil") =>
            {
                Some(name.clone())
            }
            _ => None,
        })
        .collect()
}

/// The named templates an action calls. `include` and `template` take the name as
/// their first argument; a `block` calls the template it defines, in place.
fn invocations(tokens: &[Token], kind: &ActionKind) -> Vec<String> {
    let mut out = Vec::new();
    if let ActionKind::Block { name } | ActionKind::TemplateCall { name } = kind {
        out.push(name.clone());
    }
    for (index, token) in tokens.iter().enumerate() {
        let Tok::Ident(name) = &token.kind else {
            continue;
        };
        if name != "include" && name != "template" {
            continue;
        }
        // `{{ template "x" . }}` is already accounted for by the action's kind.
        if index == 0 && matches!(kind, ActionKind::TemplateCall { .. }) {
            continue;
        }
        if let Some(Tok::Str(target)) = tokens.get(index + 1).map(|t| &t.kind) {
            if !out.contains(target) {
                out.push(target.clone());
            }
        }
    }
    out
}

// Every item above is public, and `tests/helm_template.rs` exercises all of it
// through the same door a caller uses — the parser has no private behaviour that
// an in-module test could reach and that file could not.
