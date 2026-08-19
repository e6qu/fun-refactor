//! The core vocabulary: symbols, references, scopes and imports.
//!
//! These types are shared by every language. Language-specific knowledge lives in
//! tree-sitter query files (`queries/<lang>/*.scm`), not here.

use crate::lang::Language;
use crate::span::Span;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What kind of thing a symbol is.
///
/// Imperative and config kinds share one enum so the index, rename and reference machinery stay
/// uniform across languages.
///
/// [`SymbolKind::as_str`] is this kind's identifier: it is what `--json` emits, what a
/// catalogue's `symbol_kind:` matches, and what the tables a person reads print. The serde
/// spelling has to be the same string. For three variants it was not, the tool emitted `"kind":
/// "type"` and could not read it back, which is a round trip nothing was checking. Three
/// renames below, and a test over every variant.
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
    ///
    /// Nine refusals wrote `is a {kind}`, and four kinds start with a vowel. A YAML anchor
    /// asked to be a flag was told "it is a anchor". The article belongs to the word, so it
    /// lives with the word.
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

    /// Is this a callable thing? Determines call-graph participation.
    pub fn is_callable(&self) -> bool {
        matches!(self, SymbolKind::Function | SymbolKind::Method)
    }

    /// Is this local to a function body (as opposed to file- or project-visible)?
    pub fn is_local(&self) -> bool {
        matches!(self, SymbolKind::Variable | SymbolKind::Parameter)
    }

    /// Is this kind referenced by name from other files, and not through scope
    /// or imports?
    ///
    /// CSS classes, element ids, custom properties, YAML keys, Markdown headings and
    /// link definitions are all named globally by string. This is what makes
    /// cross-language references possible: `class="btn"` in HTML names the `.btn`
    /// declared in a stylesheet.
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
    ///
    /// A CSS class has no canonical definition: `.btn` and `.btn:hover` are both definitions of
    /// the same class. A custom property can be redeclared per scope. Renaming such an entity
    /// has to rewrite every site, so these kinds are not treated as an ambiguous choice between
    /// rival definitions.
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
///
/// Every resolved edge carries one of these. Refactorings refuse to act on
/// low-confidence resolutions instead of guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Resolved by lexical scope or an unambiguous local definition. Safe to edit.
    Exact,
    /// Resolved through an explicit import or qualified path. Safe to edit.
    ImportQualified,
    /// Matched by field/member name without knowing the receiver's type.
    /// Plausible but unproven, refactorings must not silently rewrite these.
    FieldBased,
    /// Matched by bare name only. Weakest tier; report, never rewrite.
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
///
/// [`crate::refactor::Refusal::TooWeak`] means "this resolved, and not strongly enough to act
/// on". Five sites raised it where nothing had resolved at all, filling the field with
/// `Confidence::NameOnly`. So a shell script that sources a computed path was refused with
/// "resolution is only 'name-only'", sending the reader after a resolution problem that was not
/// there. Taking this instead of a bare [`Confidence`] means the variant cannot be built
/// without a reference to take it from.
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
    /// Span of just the identifier. This is what a rename edits.
    pub name_span: Span,
    /// Span of the whole definition, including body. This is what a move or delete edits.
    pub full_span: Span,
    /// Not serialized: every item in a file shares one path, which the cache restores
    /// on load. Storing it per item made entries several times larger than the source.
    #[serde(skip)]
    pub file: PathBuf,
    pub language: Language,
    /// Innermost scope containing the definition.
    pub scope: ScopeId,
    /// Enclosing symbol, e.g. the class owning a method or the function owning a local.
    pub container: Option<SymbolId>,
    /// Name of the enclosing type-like construct, used for qualification.
    ///
    /// Distinct from [`Symbol::container`] because some containers are not symbols themselves:
    /// a Rust `impl S` block qualifies its methods as `S::m`. But the `S` it names is a
    /// *reference* to the struct, not a second definition of it.
    pub qualifier: Option<String>,
    /// Whether the symbol is visible outside its file (`pub`, `export`, capitalised in Go…).
    pub exported: bool,
}

impl Symbol {
    /// Fully-qualified display name, e.g. `Type::method`.
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
    /// Span of the identifier at the use site. This is what a rename edits.
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
    /// What this reference was written against, when it was written as a member of
    /// something: the `w` in `w.contextWithTimeout(…)`, the `time` in `time.Now()`.
    ///
    /// The two are the same syntax in Go, and telling them apart decides whether the
    /// name is a method or a package-level function. Resolution can only make that
    /// call by asking whether the receiver is an import binding, which needs the
    /// receiver recorded here.
    #[serde(default)]
    pub receiver: Option<String>,
    /// Written after a `.` inside a macro's token tree, where the grammar records
    /// tokens and not syntax and so records no receiver.
    ///
    /// `assert_eq!(f.scope_at(30), …)` reaches the query as a bare identifier: not a
    /// call, and with nothing before it. Left at that it is indistinguishable from a
    /// call to a free function of the same name, which renaming the free
    /// `scope_at` rewrote, turning four method calls into calls to a method that does
    /// not exist. The dot is still in the source even where the syntax is not.
    #[serde(default)]
    pub member_in_macro: bool,
    /// The kind of declaration this reference can name, where the syntax says so.
    ///
    /// `class="thing"` names a CSS class and `href="#thing"` names an element id. Both are
    /// string references to `thing`, and without this the nearer declaration won: renaming the
    /// id `#thing` rewrote `class="thing"` as well. So the element lost the class that styled
    /// it and gained one that nothing declares.
    #[serde(default)]
    pub expects: Option<SymbolKind>,
    /// The receiver was written as a *path* — Rust's `Patterns::build`, `super::f`,
    /// and not as a value.
    ///
    /// A path names a type or a module, so it can be matched against a symbol's own
    /// qualifier without knowing any types. A value receiver names something whose
    /// type is unknown, and only a member can follow it. Conflating the two makes
    /// `super::render(…)` look like a method call on a value called `super`.
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
    /// The name written in a comment or a string, which no grammar links to the
    /// declaration. `fr usages` lists these apart. No command edits them.
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
    /// The module path as written, e.g. `std::collections::HashMap`, `./utils`.
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
    /// True when this statement also exports what it brings in:
    /// `export { width } from "./holder"`.
    ///
    /// A file like that declares nothing, so a name imported from it resolves to no
    /// definition there. The declaration is one hop further on, in the file this
    /// statement names.
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
///
/// A Markdown heading is referenced by this. It is not by its text. So it is what resolution compares
/// a `#fragment` against and what a rename writes into one.
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
    ///
    /// A method has no container, because an `impl` block is not a declaration that the
    /// index records. It has a qualifier, which is the type it belongs to. A check that
    /// asks only about the container counts a method as a top-level item of its file, and
    /// `fr move` then wrote `use crate::holder::width;` into the file it had just moved
    /// `width` into, naming a function that was no longer there.
    pub fn is_top_level(&self) -> bool {
        self.container.is_none() && self.qualifier.is_none()
    }
}

/// A reason some of a file's content is missing from the index.
///
/// A gap carries the sentence that reports it. So a new way for extraction to come up short
/// cannot be added without also deciding what the user is told about it. The alternative, a
/// bool meaning "trust this file less", has to pick one sentence for every reason, and grew
/// wrong the moment a second reason existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FactGap {
    /// The grammar produced ERROR nodes.
    SyntaxErrors,
    /// A Helm action stands where a mapping key belongs. The key has no name before
    /// the template renders, so the entry is not in the index at all.
    TemplatedKeys,
}

impl FactGap {
    /// What went missing. Each reporting site appends what that costs it there.
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
///
/// A manifest's `metadata.name` is written as a value and not as a key, so no symbol
/// carries it. Another manifest reaches this object by that name. A
/// `configMapKeyRef` names the ConfigMap it reads a key from. With the declaration
/// recorded per file, resolution can pick between four workspace `LOG_LEVEL` keys
/// instead of guessing.
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
    /// Why the file's facts are incomplete, empty when they are not. Carried with the
    /// facts so a cached entry answers the question without reparsing.
    #[serde(default)]
    pub gaps: Vec<FactGap>,
    /// Set when the file could not be read at all. So a parallel worker can report the failure
    /// through its result instead of needing a second channel.
    #[serde(skip)]
    pub unreadable: Option<String>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub scopes: Vec<Scope>,
    pub imports: Vec<Import>,
}

/// The innermost scope containing `offset`.
///
/// A free function over the scopes themselves, because two different types hold the same
/// `Vec<Scope>`, the per-file facts before resolution and the index's view of a file after it.
/// The answer must not depend on which one you happen to have.
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
