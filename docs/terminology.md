# Terminology

This file defines the terms that the code, the tests and the other documents use. Each
term has one meaning in this project. Use the term that appears here. Do not introduce a
second word for the same idea.

## Parsing

**Grammar.** A description of the syntax of one language. This project uses the grammars
published by the tree-sitter projects. The build compiles one grammar for each supported
language.

**tree-sitter.** The parser library that this project uses. It reads a source file and
builds a syntax tree. It produces a tree even when the file contains an error.

**Syntax tree.** The tree that the parser builds from a source file. The tree keeps every
byte of the source, including punctuation and whitespace. This project never rewrites the
source before it parses, so a position in the tree is a position in the file the user
wrote.

**Node.** One element of the syntax tree. A node has a kind, such as `function_item`, and
a start byte and an end byte.

**ERROR node.** A node that the parser produces where the grammar cannot read the source.
An ERROR node can cover a few bytes or a whole file. `fr parse` counts these and names
the files that contain them.

**Byte offset.** The position of a byte from the start of a file. This project measures
every position in bytes.

**Span.** A pair of byte offsets that marks a region of a file. A symbol has a name span,
which is the identifier alone, and a full span, which is the whole declaration.

**Query.** A pattern file under `queries/<language>/facts.scm` that tells the extractor
which nodes are definitions, references, scopes and imports. A query file holds all of the
knowledge about one language.

**Capture.** A name that a query attaches to a matched node, such as `@definition.function`
or `@reference.call`. The extractor reads the captures and builds facts from them.

**Mask.** A copy of a source file in which some regions are replaced with other bytes of
the same length. The parser reads the mask. Every byte offset in the tree still points at
the original file. Two languages need a mask: Helm, because a template action is not valid
YAML, and SCSS, because the grammar cannot read an interpolation in a declaration value.

**Token tree.** The body of a Rust macro call, such as the arguments of `assert_eq!`.
tree-sitter reports the tokens inside it and does not report their structure. A name inside
a token tree carries no receiver and no call.

## Facts and the index

**Fact.** One statement about a file that the extractor produced: a symbol, a reference, a
scope or an import.

**Symbol.** A declaration. A symbol has a name, a kind, a file, a name span and a full
span. Examples: a Rust function, a CSS class, a YAML key, a Markdown heading.

**Reference.** A use site of a name. A reference has a name, a span and a kind.

**Kind.** The category of a symbol or of a reference. Symbol kinds include function,
method, struct, key, selector and heading. Reference kinds are identifier, call, type,
field and string reference.

**Scope.** A region of a file in which a name is visible. Scopes nest. The innermost scope
that declares a name is the one that a reference in that scope reads.

**Shadowing.** A second declaration of a name in an inner scope. The inner declaration
hides the outer one for the region of the inner scope.

**Container.** The declaration that encloses another declaration. A function inside an
`impl` block has that type as its container, and the extractor records the type name as
the qualifier.

**Receiver.** What a member is read from. In `holder.width(2)` the receiver is `holder`.
In `Foo::new()` the receiver is `Foo`, and the project marks it as a path because it names
a type or a module.

**Member access.** A reference written after a receiver. A member access names a field or
a method. In Rust a call with no receiver is not a member access, because Rust has no
implicit self.

**Index.** The result of merging the facts from every file in a workspace. The index
resolves references to symbols.

**Resolution.** The step that decides which symbol a reference names. Resolution reads the
whole index, so it can follow a name across files.

**Target.** The symbol that a reference resolved to. A reference with no target is
unresolved.

**Confidence.** How much the resolution can be trusted. The four values, from strongest to
weakest, are exact, import-qualified, field-based and name-only. A refactoring rewrites a
reference at exact or import-qualified. It reports the others and leaves them alone.

**Fact gap.** A reason that the facts for one file are incomplete. There are two: the
grammar produced ERROR nodes, and a Helm template action stands where a key belongs. Every
command that reads an incomplete file names the gap in its output.

**Entry point.** A symbol that something outside the workspace calls: a `main` function, an
HTTP route handler, a test. The catalogs under `catalogs/` hold the rules that detect them.

**Catalog.** A YAML file of entry-point rules for one language or one ecosystem.

**Dead code.** A symbol that nothing references and that no entry point reaches. `fr unused`
lists these and states the limits of the answer.

**Call graph.** The edges between functions that call each other. `fr graph` exports it.

**Dispatch.** A call whose target depends on a value at run time, through a trait object,
an interface value or a base class. The call graph records these edges apart from resolved
calls, because a name and an arity are weaker evidence than a resolved reference.

## Refactorings

**Refactoring.** A change to source code that keeps its behaviour. Each command that
changes code produces a plan and prints it as a diff. The change reaches disk only with
`--write`.

**Plan.** The set of edits that a refactoring would apply, with the warnings that go with
them. A plan is computed and inspected before anything is written.

**Edit.** One replacement of a span with new text, with the reason recorded beside it.

**Refusal.** A refactoring that declines to proceed and states why. A refusal is a result,
not a failure. Read the reason: one refusal in this project named the wrong file for the
wrong reason for months.

**Warning.** A statement about work that a refactoring did not do, such as a reference
that resolved too weakly to rewrite. The refactoring still succeeds.

**Idempotence.** The property that running a command a second time changes nothing. It
holds for a command that normalises, such as `fr imports`.

**Inverse.** A pair of operations that return a file to its first state, such as `fr extract`
followed by `fr inline`. A failed inverse is evidence of a defect in one of the two.

## Configuration analysis

**Provenance.** The chain of sources that supply one configuration value, with the winner
marked. `fr flow` answers the same question for imperative code.

**Competition.** The set of sources that could supply one value. A competition is decided
when the workspace shows which source wins, and undecided when a channel outside the
workspace could change the answer.

**Precedence.** The rule that orders competing sources, such as the order of Helm values
files or the CSS cascade.

**Stitch.** The link from a configuration key to the code that reads it.

## Build and verification

**Feature.** A cargo build flag that includes or excludes part of the crate. The `cli`
feature adds the terminal program. The `wasm` feature builds the browser library. Code
under one feature is invisible to a build without it, so both are checked.

**Fact cache.** A store of extracted facts on disk, keyed by the bytes of a file. The key
also includes a fingerprint of the sources that decide what a fact means, so an entry
written by an older extractor is never read by a newer one.

**Corpus.** A real repository used as test input, such as `twbs/bootstrap` for SCSS or
`bitnami/charts` for Helm. A corpus measures what a change is worth.

**Sweep.** A run of one command over every candidate in a repository, with the results
counted. A sweep finds defects that a single example does not.
