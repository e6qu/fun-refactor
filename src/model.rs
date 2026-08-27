//! The core vocabulary: symbols, references, scopes and imports.

use crate::lang::Language;
use crate::span::Span;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What kind of thing a symbol is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    // Imperative
    Function,
    Method,
    Class,
    Struct,
    Trait,
    Interface,
    Enum,
    #[serde(rename = "type")]
    TypeAlias,
    Constant,
    Variable,
    Parameter,
    Field,
    Module,
    // Config and markup
    /// Terraform `resource` / `data` / `module` / `output` block, etc.
    Block,
    /// A key in a mapping (YAML/Helm values, JSON-ish structures).
    Key,
    /// CSS selector target: a class or id definition site.
    Selector,
    /// CSS custom property or SCSS variable.
    Property,
    /// YAML anchor.
    Anchor,
    /// Markdown heading.
    Heading,
    /// Markdown link reference definition.
    #[serde(rename = "link-def")]
    LinkDef,
    /// XML/HTML element id.
    #[serde(rename = "element-id")]
    ElementId,
    /// The value of a `data-*` attribute: a hook a document and the component that
    /// renders it agree on by string, `data-testid="submit-btn"`.
    #[serde(rename = "data-attribute")]
    DataAttribute,
}

impl SymbolKind {
    /// Every kind, so a rule about kinds can be asked of all of them at once.
    pub const ALL: &'static [SymbolKind] = &[
        SymbolKind::Function,
        SymbolKind::Method,
        SymbolKind::Class,
        SymbolKind::Struct,
        SymbolKind::Trait,
        SymbolKind::Interface,
        SymbolKind::Enum,
        SymbolKind::TypeAlias,
        SymbolKind::Constant,
        SymbolKind::Variable,
        SymbolKind::Parameter,
        SymbolKind::Field,
        SymbolKind::Module,
        SymbolKind::Block,
        SymbolKind::Key,
        SymbolKind::Selector,
        SymbolKind::Property,
        SymbolKind::Anchor,
        SymbolKind::Heading,
        SymbolKind::LinkDef,
        SymbolKind::ElementId,
        SymbolKind::DataAttribute,
    ];

    /// The kind with the article that fits it, for a sentence that reads.
    pub fn with_article(&self) -> String {
        let word = self.as_str();
        let article = match word.starts_with(['a', 'e', 'i', 'o', 'u']) {
            true => "an",
            false => "a",
        };
        format!("{article} {word}")
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::Enum => "enum",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Constant => "constant",
            SymbolKind::Variable => "variable",
            SymbolKind::Parameter => "parameter",
            SymbolKind::Field => "field",
            SymbolKind::Module => "module",
            SymbolKind::Block => "block",
            SymbolKind::Key => "key",
            SymbolKind::Selector => "selector",
            SymbolKind::Property => "property",
            SymbolKind::Anchor => "anchor",
            SymbolKind::Heading => "heading",
            SymbolKind::LinkDef => "link-def",
            SymbolKind::ElementId => "element-id",
            SymbolKind::DataAttribute => "data-attribute",
        }
    }

    /// Is this a callable thing?
    pub fn is_callable(&self) -> bool {
        matches!(self, SymbolKind::Function | SymbolKind::Method)
    }

    /// Is this local to a function body (as opposed to file- or project-visible)?
    pub fn is_local(&self) -> bool {
        matches!(self, SymbolKind::Variable | SymbolKind::Parameter)
    }

    /// Is this kind referenced by name from other files, and not through scope or imports?
    pub fn is_string_keyed(&self) -> bool {
        matches!(
            self,
            SymbolKind::Selector
                | SymbolKind::ElementId
                | SymbolKind::Property
                | SymbolKind::Heading
                | SymbolKind::LinkDef
                | SymbolKind::Anchor
                | SymbolKind::DataAttribute
        )
    }

    /// May one entity of this kind legitimately have many definition sites?
    pub fn allows_multiple_definitions(&self) -> bool {
        matches!(
            self,
            SymbolKind::Selector
                | SymbolKind::Property
                | SymbolKind::ElementId
                | SymbolKind::DataAttribute
        )
    }
}

/// How confident we are that a reference or call resolved to the right symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Resolved by lexical scope or an unambiguous local definition.
    Exact,
    /// Resolved through an explicit import or qualified path.
    ImportQualified,
    /// Matched by field/member name without knowing the receiver's type.
    FieldBased,
    /// Matched by bare name only.
    NameOnly,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Exact => "exact",
            Confidence::ImportQualified => "import-qualified",
            Confidence::FieldBased => "field-based",
            Confidence::NameOnly => "name-only",
        }
    }

    /// May a mutating refactoring rewrite this reference without confirmation?
    pub fn is_safe_to_rewrite(&self) -> bool {
        matches!(self, Confidence::Exact | Confidence::ImportQualified)
    }
}

/// The confidence of a reference that exists, which only a reference can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedConfidence(Confidence);

impl ResolvedConfidence {
    pub fn get(self) -> Confidence {
        self.0
    }
}

/// Identifies a symbol within a workspace index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

/// Identifies a scope within one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeId(pub u32);

/// A definition site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    /// Span of just the identifier.
    pub name_span: Span,
    /// Span of the whole definition, including body.
    pub full_span: Span,
    /// Not serialized: every item in a file shares one path, which the cache restores on load.
    #[serde(skip)]
    pub file: PathBuf,
    pub language: Language,
    /// Innermost scope containing the definition.
    pub scope: ScopeId,
    /// Enclosing symbol, e.g.
    pub container: Option<SymbolId>,
    /// Name of the enclosing type-like construct, used for qualification.
    pub qualifier: Option<String>,
    /// Whether the symbol is visible outside its file (`pub`, `export`, capitalised in Go…).
    pub exported: bool,
}

impl Symbol {
    /// Fully-qualified display name, e.g.
    pub fn qualified_name(&self) -> String {
        match &self.qualifier {
            Some(q) => format!("{q}::{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// A use site of some name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub name: String,
    /// Span of the identifier at the use site.
    pub span: Span,
    /// Not serialized; see [`Symbol::file`].
    #[serde(skip)]
    pub file: PathBuf,
    pub language: Language,
    /// Innermost scope containing the reference.
    pub scope: ScopeId,
    /// The symbol this resolved to, if resolution succeeded.
    pub target: Option<SymbolId>,
    pub confidence: Confidence,
    pub kind: ReferenceKind,
    /// A second meaning sharing another reference's span: a shorthand initializer's identifier
    /// reads a local and writes a field, and this is the field half.
    #[serde(default)]
    pub twin: bool,
    /// What this reference was written against, when it was written as a member of something:
    /// the `w` in `w.contextWithTimeout(…)`, the `time` in `time.Now()`.
    #[serde(default)]
    pub receiver: Option<String>,
    /// Written after a `.` inside a macro's token tree, where the grammar records tokens and
    /// not syntax and so records no receiver.
    #[serde(default)]
    pub member_in_macro: bool,
    /// The kind of declaration this reference can name, where the syntax says so.
    #[serde(default)]
    pub expects: Option<SymbolKind>,
    /// The receiver was written as a *path*, as in Rust's `Patterns::build`, `super::f`, and
    /// not as a value.
    #[serde(default)]
    pub receiver_is_path: bool,
}

impl Reference {
    /// This reference's confidence, in the form a refusal requires.
    pub fn resolved_confidence(&self) -> ResolvedConfidence {
        ResolvedConfidence(self.confidence)
    }
}

/// What syntactic role a reference plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceKind {
    /// A plain identifier use.
    Identifier,
    /// The callee of a call expression.
    Call,
    /// A type position.
    Type,
    /// A field or member access.
    Field,
    /// A reference inside a string, template or attribute (Helm `.Values.x`,
    /// HTML `class="btn"`, Markdown `#anchor`).
    StringRef,
    /// The name written in a comment or a string, which no grammar links to the declaration.
    Textual,
}

/// A lexical scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub id: ScopeId,
    pub span: Span,
    pub parent: Option<ScopeId>,
}

/// An import / include / use statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// The module path as written, e.g.
    pub path: String,
    /// Local binding introduced, if any (`use x as y` binds `y`).
    pub alias: Option<String>,
    /// The specific names imported, for languages with named imports.
    pub names: Vec<ImportedName>,
    pub span: Span,
    /// Not serialized; see [`Symbol::file`].
    #[serde(skip)]
    pub file: PathBuf,
    /// True for glob imports (`use x::*`, `from m import *`), which make
    /// name resolution ambiguous and force a confidence downgrade.
    pub is_glob: bool,
    /// True when this statement also exports what it brings in: `export { width } from
    /// "./holder"`.
    #[serde(default)]
    pub re_export: bool,
}

/// One name bound by an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedName {
    /// Name as exported by the source module.
    pub original: String,
    /// Name bound locally; differs from `original` when aliased.
    pub local: String,
    pub span: Span,
}

impl ImportedName {
    pub fn is_aliased(&self) -> bool {
        self.original != self.local
    }
}

/// GitHub's heading anchor: lowercased, punctuation dropped, spaces hyphenated.
pub fn anchor_slug(heading: &str) -> String {
    let mut out = String::with_capacity(heading.len());
    for ch in heading.trim().chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' || ch == '_' {
            out.push(if ch == '_' { '_' } else { '-' });
        }
    }
    out
}

impl Symbol {
    /// Can another file name this declaration on its own, with no type in front of it?
    pub fn is_top_level(&self) -> bool {
        self.container.is_none() && self.qualifier.is_none()
    }
}

/// A reason some of a file's content is missing from the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FactGap {
    /// The grammar produced ERROR nodes.
    SyntaxErrors,
    /// A Helm action stands where a mapping key belongs.
    TemplatedKeys,
}

impl FactGap {
    /// What went missing.
    pub fn cause(self) -> &'static str {
        match self {
            Self::SyntaxErrors => "file has syntax errors",
            Self::TemplatedKeys => "file has a template action where a key belongs",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SyntaxErrors => "syntax-errors",
            Self::TemplatedKeys => "templated-keys",
        }
    }
}

/// A Kubernetes object a file declares, addressed by name from another file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KubernetesObject {
    /// The `kind` field as written: `ConfigMap`, `Secret`.
    pub kind: String,
    /// The `metadata.name` field as written.
    pub name: String,
    /// The name value's own bytes, which a rename of the object would rewrite.
    pub name_span: Span,
}

/// Everything extracted from one file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileFacts {
    pub path: PathBuf,
    /// The Kubernetes objects this file declares, in document order.
    #[serde(default)]
    pub kubernetes_objects: Vec<KubernetesObject>,
    /// Why the file's facts are incomplete, empty when they are not.
    #[serde(default)]
    pub gaps: Vec<FactGap>,
    /// Set when the file could not be read at all.
    #[serde(skip)]
    pub unreadable: Option<String>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub scopes: Vec<Scope>,
    pub imports: Vec<Import>,
}

/// The innermost scope containing `offset`.
pub fn scope_at(scopes: &[Scope], offset: usize) -> Option<ScopeId> {
    scopes
        .iter()
        .filter(|s| s.span.contains_offset(offset))
        .min_by_key(|s| s.span.len())
        .map(|s| s.id)
}

/// Walk from `scope` outwards to the file root.
pub fn scope_chain(scopes: &[Scope], scope: ScopeId) -> Vec<ScopeId> {
    let mut chain = vec![scope];
    let mut current = scope;
    // Scope parents form a tree; the bound guards against a malformed cycle.
    for _ in 0..scopes.len() {
        let Some(parent) = scopes
            .iter()
            .find(|s| s.id == current)
            .and_then(|s| s.parent)
        else {
            break;
        };
        chain.push(parent);
        current = parent;
    }
    chain
}

impl FileFacts {
    /// The innermost scope containing `offset`.
    pub fn scope_at(&self, offset: usize) -> Option<ScopeId> {
        scope_at(&self.scopes, offset)
    }

    /// Walk from `scope` outwards to the file root.
    pub fn scope_chain(&self, scope: ScopeId) -> Vec<ScopeId> {
        scope_chain(&self.scopes, scope)
    }

    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.id == id)
    }

    /// Symbol whose name span contains `offset`, if the offset is on a definition.
    pub fn symbol_at(&self, offset: usize) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| s.name_span.contains_offset(offset))
    }

    /// Reference whose span contains `offset`.
    pub fn reference_at(&self, offset: usize) -> Option<&Reference> {
        self.references
            .iter()
            .find(|r| r.span.contains_offset(offset))
    }
}

#[cfg(test)]
mod article_tests {
    use super::SymbolKind;

    #[test]
    fn a_kind_starting_with_a_vowel_takes_an() {
        // Nine refusals wrote the article themselves and four kinds start with a vowel,
        // so `fr inline` on a Java interface said "is a interface".
        assert_eq!(SymbolKind::Interface.with_article(), "an interface");
        assert_eq!(SymbolKind::Anchor.with_article(), "an anchor");
        assert_eq!(SymbolKind::Function.with_article(), "a function");
        assert_eq!(SymbolKind::Variable.with_article(), "a variable");
    }

    #[test]
    fn every_kind_gets_the_article_its_first_letter_asks_for() {
        for kind in SymbolKind::ALL {
            let said = kind.with_article();
            let vowel = kind.as_str().starts_with(['a', 'e', 'i', 'o', 'u']);
            assert_eq!(
                said.starts_with("an "),
                vowel,
                "{} takes the wrong article: {said}",
                kind.as_str()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(id: u32, span: (usize, usize), parent: Option<u32>) -> Scope {
        Scope {
            id: ScopeId(id),
            span: Span::new(span.0, span.1),
            parent: parent.map(ScopeId),
        }
    }

    fn facts() -> FileFacts {
        FileFacts {
            path: PathBuf::from("a.rs"),
            scopes: vec![
                scope(0, (0, 100), None),
                scope(1, (10, 60), Some(0)),
                scope(2, (20, 40), Some(1)),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn scope_at_picks_the_innermost() {
        let f = facts();
        assert_eq!(f.scope_at(30), Some(ScopeId(2)));
        assert_eq!(f.scope_at(50), Some(ScopeId(1)));
        assert_eq!(f.scope_at(90), Some(ScopeId(0)));
        assert_eq!(f.scope_at(200), None);
    }

    #[test]
    fn scope_chain_walks_outward_to_root() {
        let f = facts();
        assert_eq!(
            f.scope_chain(ScopeId(2)),
            vec![ScopeId(2), ScopeId(1), ScopeId(0)]
        );
        assert_eq!(f.scope_chain(ScopeId(0)), vec![ScopeId(0)]);
    }

    #[test]
    fn scope_chain_terminates_on_a_cycle() {
        // A malformed scope tree must not hang the tool.
        let f = FileFacts {
            scopes: vec![scope(0, (0, 10), Some(1)), scope(1, (0, 10), Some(0))],
            ..Default::default()
        };
        assert!(f.scope_chain(ScopeId(0)).len() <= 3);
    }

    #[test]
    fn confidence_ordering_reflects_trust() {
        assert!(Confidence::Exact < Confidence::ImportQualified);
        assert!(Confidence::ImportQualified < Confidence::FieldBased);
        assert!(Confidence::FieldBased < Confidence::NameOnly);
        assert!(Confidence::Exact.is_safe_to_rewrite());
        assert!(Confidence::ImportQualified.is_safe_to_rewrite());
        // Anything weaker must not be rewritten silently.
        assert!(!Confidence::FieldBased.is_safe_to_rewrite());
        assert!(!Confidence::NameOnly.is_safe_to_rewrite());
    }

    #[test]
    fn imported_name_detects_aliasing() {
        let plain = ImportedName {
            original: "foo".into(),
            local: "foo".into(),
            span: Span::new(0, 3),
        };
        let aliased = ImportedName {
            original: "foo".into(),
            local: "bar".into(),
            span: Span::new(0, 3),
        };
        assert!(!plain.is_aliased());
        assert!(aliased.is_aliased());
    }

    #[test]
    fn symbol_kind_classification() {
        assert!(SymbolKind::Function.is_callable());
        assert!(SymbolKind::Method.is_callable());
        assert!(!SymbolKind::Struct.is_callable());
        assert!(SymbolKind::Variable.is_local());
        assert!(!SymbolKind::Function.is_local());
    }
}
