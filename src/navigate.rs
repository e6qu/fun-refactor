//! Navigation: go to definition, go to usages, go to implementations.
//!
//! These are the three questions an editor asks. The reason they are one module is that each
//! answer is a *set*. It is not a single site. A trait method has as many definitions as it has
//! implementations. A CSS class is declared by every rule that names it. A Terraform local is
//! one definition read from many files.
//!
//! So every answer here is a list, each entry carrying the confidence of the resolution that
//! produced it. Callers decide how much of the tail to show instead of being handed one result
//! that looks certain.

use crate::analysis::call_graph::Hierarchy;
use crate::index::Index;
use crate::model::{Confidence, ReferenceKind, Symbol, SymbolId, SymbolKind};
use crate::span::{LineIndex, Span};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A place to go.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Location {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    pub span: Span,
    /// The source line, trimmed, for a preview.
    pub preview: String,
}

/// One definition of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Definition {
    pub symbol: SymbolId,
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub location: Location,
    /// Why this is being offered as a definition.
    pub role: DefinitionRole,
}

/// How a definition relates to what was asked about.
///
/// Serialised as a token instead of the variant name, so `--json` spells it the way
/// every other enum here does and the browser does not have to know Rust's naming.
/// The prose the terminal prints is [`DefinitionRole::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefinitionRole {
    /// The definition the reference resolves to.
    Primary,
    /// Another site declaring the same entity, a second CSS rule for one class.
    SameEntity,
    /// A concrete implementation of the abstract thing asked about.
    Implementation,
}

impl DefinitionRole {
    /// Prose for a reader. See the note on `Basis::describe`.
    pub fn label(&self) -> &'static str {
        match self {
            DefinitionRole::Primary => "definition",
            DefinitionRole::SameEntity => "also declared here",
            DefinitionRole::Implementation => "implementation",
        }
    }
}

/// One use of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Usage {
    pub location: Location,
    pub kind: ReferenceKind,
    pub confidence: Confidence,
    /// The enclosing function or type, when there is one, the context a reader wants.
    pub within: Option<String>,
}

/// Everything known about a symbol's definitions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Definitions {
    pub query: String,
    pub definitions: Vec<Definition>,
}

impl Definitions {
    /// Is this an abstraction with more than one concrete answer?
    pub fn is_polymorphic(&self) -> bool {
        self.definitions
            .iter()
            .filter(|d| d.role == DefinitionRole::Implementation)
            .count()
            > 0
    }

    pub fn primary(&self) -> Option<&Definition> {
        self.definitions
            .iter()
            .find(|d| d.role == DefinitionRole::Primary)
    }
}

/// Everything known about a symbol's uses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Usages {
    pub query: String,
    pub usages: Vec<Usage>,
    /// Occurrences sharing the name that resolved elsewhere or not at all. Reported
    /// separately so a caller never mistakes them for uses of this symbol.
    pub same_name_elsewhere: Vec<Usage>,
    /// The name written in a comment or a string. Nothing resolves these, and no
    /// command edits them, so they are listed apart from the references.
    pub in_text: Vec<Usage>,
}

impl Usages {
    pub fn certain(&self) -> Vec<&Usage> {
        self.usages
            .iter()
            .filter(|u| u.confidence.is_safe_to_rewrite())
            .collect()
    }

    /// Usages grouped by file, in path order.
    pub fn by_file(&self) -> BTreeMap<&PathBuf, Vec<&Usage>> {
        let mut grouped: BTreeMap<&PathBuf, Vec<&Usage>> = BTreeMap::new();
        for usage in &self.usages {
            grouped.entry(&usage.location.file).or_default().push(usage);
        }
        grouped
    }
}

/// Build a location from a file and byte offset.
fn locate(file: &Path, span: Span) -> Location {
    let source = crate::vfs::read_to_string(file).unwrap_or_default();
    let index = LineIndex::new(&source);
    let pos = index.line_col(span.start, &source);
    let preview = index
        .line_span(pos.line)
        .map(|l| l.text(&source).trim().to_string())
        .unwrap_or_default();
    Location {
        file: file.to_path_buf(),
        line: pos.line,
        col: pos.col,
        span,
        preview,
    }
}

fn definition_of(symbol: &Symbol, role: DefinitionRole) -> Definition {
    Definition {
        symbol: symbol.id,
        name: symbol.name.clone(),
        qualified_name: symbol.qualified_name(),
        kind: symbol.kind,
        location: locate(&symbol.file, symbol.name_span),
        role,
    }
}

/// Every definition the thing at `offset` could refer to.
///
/// A position on a call to a trait method yields the trait's declaration *and* every
/// implementation, because "where is this defined" has no single answer there.
pub fn definitions_at(index: &Index, file: &Path, offset: usize) -> Option<Definitions> {
    let symbol = index.definition_at(file, offset)?;
    Some(definitions_of(index, symbol.id))
}

/// Every definition of a known symbol.
pub fn definitions_of(index: &Index, symbol_id: SymbolId) -> Definitions {
    definitions_with(&Hierarchy::scanned(index), index, symbol_id)
}

/// [`definitions_of`] against an already-scanned hierarchy.
pub fn definitions_with(hierarchy: &Hierarchy, index: &Index, symbol_id: SymbolId) -> Definitions {
    let Some(symbol) = index.symbol(symbol_id) else {
        return Definitions {
            query: String::new(),
            definitions: Vec::new(),
        };
    };

    let mut definitions = vec![definition_of(symbol, DefinitionRole::Primary)];

    // Other sites declaring the same entity: a CSS class written by several rules.
    for other in index.definition_group(symbol_id) {
        if other == symbol_id {
            continue;
        }
        if let Some(sibling) = index.symbol(other) {
            definitions.push(definition_of(sibling, DefinitionRole::SameEntity));
        }
    }

    // Concrete implementations, when the thing asked about is an abstraction.
    for implementation in implementations_with(hierarchy, index, symbol_id) {
        if let Some(concrete) = index.symbol(implementation) {
            definitions.push(definition_of(concrete, DefinitionRole::Implementation));
        }
    }

    definitions.sort_by(|a, b| {
        (a.role, &a.location.file, a.location.line).cmp(&(
            b.role,
            &b.location.file,
            b.location.line,
        ))
    });
    definitions.dedup_by_key(|d| d.symbol);

    Definitions {
        query: symbol.qualified_name(),
        definitions,
    }
}

/// Concrete implementations of an abstract declaration.
///
/// This is the same question the call graph asks at a dispatch site, answered through
/// the same [`Hierarchy`], a Rust `impl Trait for Type`, a Go interface whose method
/// set a type covers, a TypeScript `implements` clause, a Python base class. Sharing
/// it means navigation and the graph cannot disagree about what implements what.
///
/// Scanning the hierarchy costs a parse per file, so a caller answering many
/// questions should scan once and use [`implementations_with`].
pub fn implementations_of(index: &Index, symbol_id: SymbolId) -> Vec<SymbolId> {
    implementations_with(&Hierarchy::scanned(index), index, symbol_id)
}

/// [`implementations_of`] against an already-scanned hierarchy.
pub fn implementations_with(
    hierarchy: &Hierarchy,
    index: &Index,
    symbol_id: SymbolId,
) -> Vec<SymbolId> {
    hierarchy.implementations_of(index, symbol_id)
}

/// Every use of a symbol, plus the same-named occurrences that are not uses of it.
pub fn usages_of(index: &Index, symbol_id: SymbolId) -> Usages {
    if let Some(symbol) = index.symbol(symbol_id) {
        crate::capabilities::record(crate::capabilities::Capability::Symbols, symbol.language);
    }
    let query = index
        .symbol(symbol_id)
        .map(|s| s.qualified_name())
        .unwrap_or_default();

    // A polymorphic declaration is used through its implementations too.
    let mut targets = vec![symbol_id];
    targets.extend(index.definition_group(symbol_id));
    targets.extend(implementations_of(index, symbol_id));
    targets.sort();
    targets.dedup();

    let mut usages: Vec<Usage> = Vec::new();
    for target in &targets {
        for reference in index.references_to(*target) {
            usages.push(Usage {
                location: locate(&reference.file, reference.span),
                kind: reference.kind,
                confidence: reference.confidence,
                within: enclosing_name(index, &reference.file, reference.span.start),
            });
        }
    }
    usages.sort_by(|a, b| {
        (&a.location.file, a.location.line, a.location.col).cmp(&(
            &b.location.file,
            b.location.line,
            b.location.col,
        ))
    });
    usages.dedup_by(|a, b| a.location == b.location);

    // A call on a value whose type is not tracked resolves to none of the members that answer
    // to the name, `c.area()` against a trait declaration and every implementation of it. Where
    // the ambiguity is *among the things asked about*, that is a use of them, carrying the
    // confidence that says so. It is only a coincidence of naming when the symbol has no
    // implementations to be confused with.
    let polymorphic = targets.len() > 1;
    let mut same_name_elsewhere = Vec::new();
    for reference in index.unresolved_matching(symbol_id) {
        if reference.target.is_some() {
            continue;
        }
        let usage = Usage {
            location: locate(&reference.file, reference.span),
            kind: reference.kind,
            confidence: reference.confidence,
            within: enclosing_name(index, &reference.file, reference.span.start),
        };
        let member_shaped = matches!(
            reference.kind,
            crate::model::ReferenceKind::Field | crate::model::ReferenceKind::Call
        ) && reference.confidence == Confidence::FieldBased;
        if polymorphic && member_shaped {
            usages.push(usage);
        } else {
            same_name_elsewhere.push(usage);
        }
    }
    usages.sort_by(|a, b| {
        (&a.location.file, a.location.line, a.location.col).cmp(&(
            &b.location.file,
            b.location.line,
            b.location.col,
        ))
    });
    usages.dedup_by(|a, b| a.location == b.location);

    // The name found by reading the files as text. No grammar links these to
    // the declaration, so no resolver finds them, and a reader asking "where
    // does this name appear" still wants them. Reported apart, and never
    // counted as a use.
    //
    // A site this search already accounts for is not one of them. The sweep
    // matched the declaration itself and every resolved use, so the listing
    // repeated what it had counted. The heading then called a YAML key a
    // comment.
    let name = index
        .symbol(symbol_id)
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let accounted: std::collections::HashSet<(PathBuf, usize, usize)> = usages
        .iter()
        .chain(same_name_elsewhere.iter())
        .map(|u| (u.location.file.clone(), u.location.line, u.location.col))
        .chain(
            definitions_of(index, symbol_id)
                .definitions
                .iter()
                .map(|d| (d.location.file.clone(), d.location.line, d.location.col)),
        )
        .collect();
    let in_text = match name.is_empty() {
        true => Vec::new(),
        false => crate::mentions::of(index, &name)
            .unwrap_or_default()
            .into_iter()
            .map(|m| Usage {
                location: locate(&m.file, m.span),
                kind: crate::model::ReferenceKind::Textual,
                confidence: Confidence::NameOnly,
                within: enclosing_name(index, &m.file, m.span.start),
            })
            .filter(|u| {
                !accounted.contains(&(u.location.file.clone(), u.location.line, u.location.col))
            })
            .collect(),
    };

    Usages {
        query,
        usages,
        same_name_elsewhere,
        in_text,
    }
}

/// The innermost named definition containing an offset, for context.
fn enclosing_name(index: &Index, file: &Path, offset: usize) -> Option<String> {
    let info = index.file(file)?;
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.full_span.contains_offset(offset) && !s.kind.is_local())
        .min_by_key(|s| s.full_span.len())
        .map(|s| s.qualified_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{scan, ScanOptions};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            crate::vfs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    const TRAIT_SRC: &str = "\
trait Shape {
    fn area(&self) -> f64;
}

struct Circle;
impl Shape for Circle {
    fn area(&self) -> f64 { 3.0 }
}

struct Square;
impl Shape for Square {
    fn area(&self) -> f64 { 4.0 }
}
";

    #[test]
    fn a_trait_method_has_one_definition_per_implementation() {
        let (_tmp, index) = workspace(&[("a.rs", TRAIT_SRC)]);
        let declaration = index
            .find_symbols("area", None)
            .into_iter()
            .find(|s| s.qualifier.as_deref() == Some("Shape"))
            .expect("the trait declares area");

        let found = definitions_of(&index, declaration.id);
        assert!(found.is_polymorphic(), "got {:?}", found.definitions);

        let implementations: Vec<&str> = found
            .definitions
            .iter()
            .filter(|d| d.role == DefinitionRole::Implementation)
            .map(|d| d.qualified_name.as_str())
            .collect();
        assert_eq!(implementations, vec!["Circle::area", "Square::area"]);
    }

    #[test]
    fn a_plain_function_has_exactly_one_definition() {
        let (_tmp, index) = workspace(&[("a.rs", "fn solo() {}\nfn main() { solo(); }\n")]);
        let solo = index.find_symbols("solo", None)[0].id;
        let found = definitions_of(&index, solo);
        assert_eq!(found.definitions.len(), 1);
        assert!(!found.is_polymorphic());
        assert_eq!(found.primary().unwrap().qualified_name, "solo");
    }

    #[test]
    fn an_entity_declared_twice_reports_both_sites() {
        // A CSS class has no canonical definition; both rules declare it.
        let (_tmp, index) = workspace(&[(
            "a.css",
            ".btn { color: red; }\n.btn:hover { color: blue; }\n",
        )]);
        let btn = index.find_symbols("btn", None)[0].id;
        let found = definitions_of(&index, btn);
        assert_eq!(found.definitions.len(), 2, "got {:?}", found.definitions);
        assert!(found
            .definitions
            .iter()
            .any(|d| d.role == DefinitionRole::SameEntity));
    }

    #[test]
    fn a_method_on_a_concrete_type_is_not_treated_as_polymorphic() {
        let (_tmp, index) =
            workspace(&[("a.rs", "struct S;\nimpl S {\n    fn only(&self) {}\n}\n")]);
        let only = index.find_symbols("only", None)[0].id;
        assert!(implementations_of(&index, only).is_empty());
    }

    #[test]
    fn definitions_can_be_found_from_a_position() {
        let src = "fn target() {}\nfn caller() { target(); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let at_use = definitions_at(&index, &path, src.rfind("target").unwrap() + 1)
            .expect("a call position resolves");
        assert_eq!(at_use.primary().unwrap().name, "target");
        assert_eq!(at_use.primary().unwrap().location.line, 1);
    }

    #[test]
    fn usages_carry_their_context_and_confidence() {
        let src = "fn helper() {}\nfn caller() {\n    helper();\n}\n";
        let (_tmp, index) = workspace(&[("a.rs", src)]);
        let helper = index.find_symbols("helper", None)[0].id;

        let found = usages_of(&index, helper);
        assert_eq!(found.usages.len(), 1);
        assert_eq!(found.usages[0].within.as_deref(), Some("caller"));
        assert_eq!(found.usages[0].confidence, Confidence::Exact);
        assert_eq!(found.usages[0].location.preview, "helper();");
        assert_eq!(found.certain().len(), 1);
    }

    #[test]
    fn usages_of_a_trait_method_include_calls_on_implementations() {
        let mut src = TRAIT_SRC.to_string();
        src.push_str("fn use_it(c: Circle) { c.area(); }\n");
        let (_tmp, index) = workspace(&[("a.rs", &src)]);

        let declaration = index
            .find_symbols("area", None)
            .into_iter()
            .find(|s| s.qualifier.as_deref() == Some("Shape"))
            .unwrap();
        let found = usages_of(&index, declaration.id);
        assert!(
            !found.usages.is_empty(),
            "a call through an implementation is a use of the declaration"
        );
    }

    #[test]
    fn same_named_occurrences_elsewhere_are_kept_separate() {
        let (_tmp, index) = workspace(&[
            ("a.rs", "fn parse() {}\nfn a() { parse(); }\n"),
            ("b.rs", "fn other() { parse(); }\n"),
        ]);
        let parse = index.find_symbols("parse", None)[0].id;
        let found = usages_of(&index, parse);

        // The call in a.rs is a use; anything that did not resolve here is listed
        // apart, so a caller never mistakes it for one.
        assert!(found
            .usages
            .iter()
            .all(|u| u.location.file.ends_with("a.rs")));
        for usage in &found.same_name_elsewhere {
            assert!(!usage.confidence.is_safe_to_rewrite());
        }
    }

    #[test]
    fn usages_group_by_file_in_path_order() {
        let (_tmp, index) = workspace(&[
            ("lib.rs", "pub fn shared() {}\n"),
            ("a.rs", "use lib::shared;\nfn x() { shared(); }\n"),
            ("b.rs", "use lib::shared;\nfn y() { shared(); }\n"),
        ]);
        let shared = index.find_symbols("shared", None)[0].id;
        let found = usages_of(&index, shared);
        let grouped = found.by_file();
        assert!(grouped.len() >= 2, "got {grouped:?}");

        // The name of this test says "in path order" and for a long time it checked only that
        // there was more than one group. A function returning them in whatever order the hash
        // map felt like would have passed. The order is the whole reason a caller groups
        // instead of reading the flat list.
        let paths: Vec<&std::path::Path> = grouped.keys().map(|path| path.as_path()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(
            paths, sorted,
            "usages came back out of path order: {paths:?}"
        );
    }
}
