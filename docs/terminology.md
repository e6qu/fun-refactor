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

**Span.** A pair of byte offsets that marks a region of a file. A symbol carries two: its
name span covers the identifier alone, and its full span covers the whole declaration.

**Query.** A pattern file under `queries/<language>/facts.scm` that tells the extractor
which nodes are definitions, references, scopes and imports. A query file holds all of the
knowledge about one language.

**Capture.** A name that a query attaches to a matched node, such as `@definition.function`
or `@reference.call`. The extractor reads the captures and builds facts from them.

**Mask.** A copy of a source file that replaces some regions with other bytes of the same
length. The parser reads the mask. Every byte offset in the tree still points at the
original file. Two languages need a mask. Helm needs one because a template action is not
valid YAML, and SCSS because the grammar cannot read an interpolation in a declaration
value.

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

**Scope.** A region of a file in which a name is visible. Scopes nest. A reference reads
the innermost scope that declares its name.

**Shadowing.** A second declaration of a name in an inner scope. The inner declaration
hides the outer one for the region of the inner scope.

**Container.** The declaration that encloses another declaration. A function inside an
`impl` block takes that type as its container, and the extractor records the type name as
the qualifier.

**Receiver.** What a member access reads from. In `holder.width(2)` the receiver is
`holder`. In `Foo::new()` the receiver is `Foo`, and the project marks it as a path
because it names a type or a module.

**Member access.** A reference written after a receiver. A member access names a field or
a method. Rust has no implicit self, so a Rust call with no receiver is not a member
access.

**Index.** The result of merging the facts from every file in a workspace. The index
resolves references to symbols.

**Resolution.** The step that decides which symbol a reference names. Resolution reads the
whole index, so it can follow a name across files.

**Target.** The symbol that a reference resolved to. A reference with no target is
unresolved.

**Confidence.** How far a reader can trust one resolution. The four values, from strongest
to weakest, are exact, import-qualified, field-based and name-only. A refactoring rewrites
a reference at exact or import-qualified. It reports the others and leaves them alone.

**Fact gap.** A reason that the facts for one file are incomplete. There are two: the
grammar produced ERROR nodes, and a Helm template action stands where a key belongs. Every
command that reads an incomplete file names the gap in its output.

**Entry point.** A symbol that something outside the workspace calls: a `main` function, an
HTTP route handler, a test. The catalogs under `catalogs/` hold the rules that detect them.

**Catalog.** A YAML file of entry-point rules for one language or one ecosystem.

**Dead code.** A symbol that nothing references and that no entry point reaches. `fr unused`
lists these and states the limits of the answer.

**Call graph.** The edges between functions that call each other. `fr graph` exports it.

**Dispatch.** A call whose target depends on a value while the program runs, through a
trait object, an interface value or a base class. The tool cannot say which implementation
the call reaches, so it lists every implementation that the declaration could reach. The
call graph keeps these edges apart from resolved calls, because a matching name and
argument count are weaker evidence than a resolved reference.

## Refactorings

**Refactoring.** A change to source code that keeps its behaviour. Each command that
changes code produces a plan and prints it as a diff. The change reaches disk only with
`--write`.

**Plan.** The set of edits that a refactoring would apply, with the warnings that go with
them. The tool computes the plan first, and you read it before anything is written.

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

**Competition.** The set of sources that could supply one value. The workspace decides a
competition when it shows which source wins. A channel outside the workspace that could
change the answer leaves the competition undecided.

**Precedence.** The rule that orders competing sources, such as the order of Helm values
files or the CSS cascade.

**Stitch.** The link from a configuration key to the code that reads it.

## Build and verification

**Feature.** A cargo build flag that includes or excludes part of the crate. The `cli`
feature adds the terminal program. The `wasm` feature builds the browser library. Code
under one feature is invisible to a build without it, so the project checks both builds.

**Fact cache.** A store of extracted facts on disk, keyed by the bytes of a file. The key
also includes a fingerprint of the sources that decide what a fact means. A newer
extractor therefore never reads an entry that an older extractor wrote.

**Corpus.** A real repository used as test input, such as `twbs/bootstrap` for SCSS or
`bitnami/charts` for Helm. A corpus measures what a change is worth.

**Sweep.** A run of one command over every candidate in a repository, with the results
counted. A sweep finds the defects that one example misses.

**Compile gate.** A test that applies a refactoring to a copy of a workspace and then
runs the real compiler on the result. A plan that parses but does not compile fails the
gate. `tests/output_compiles.rs` and three sibling files hold these tests.

**Validator.** The tool that judges a language with no compiler, such as `bash -n` for
shell, `terraform validate` for Terraform and `xmllint` for XML. A validator plays the
part of the compiler in the gate.

**Toolchain.** The compiler or validator for one language, and the check that says
whether the machine has it. A missing toolchain skips a gate test. On CI it fails the
run, because a skipped check reads the same as a passed one.

**Capability.** One thing the tool can do to one language, such as "rename a symbol" or
"organise imports". `fr capabilities` lists them.

**Capability matrix.** The table of every capability against every language. A cell holds
either a mark for supported or `n/a` with the reason. Each cell comes from the predicate
that the command itself asks, so the table and the command cannot disagree.

**Cascade.** The repeated rounds that `fr remove-flag` runs. Round one replaces the flag
with its value. Later rounds collapse conditionals that are now constant and delete what
nothing reads. The rounds repeat until nothing changes.

**Recipe.** A file that names what to find, what to do to it, and what must be true
afterwards. `fr recipe` runs one.

**Translate.** Rewrite a file as another language. The output is a draft, and it names
every construct that did not carry across.

**Intermediary language.** The one vocabulary every translation crosses. A reader turns
a source file into it and a writer turns it back into source. So a language needs one
reader and one writer rather than a translator per pair. [IR.md](../IR.md) documents it.

**Carry verbatim.** What a writer does with a construct its target cannot spell. The
source text goes into the output under a marker. A translation with one of these is
incomplete. Dropping the construct instead would leave a file that compiles and does
something else.

**Fidelity report.** What a translation managed and what it did not, printed with the
diff and repeated at the top of the file it wrote. It counts declarations that crossed
and signatures by how completely they carried. One note per construct that did not.

**Playground.** The browser build of the tool, served from `docs/playground`. It uses
the same library as the terminal program, compiled to WebAssembly.

**Budget file.** A file that holds a count that may only go down, such as
`tools/PROSE-DEBT`. A check fails when the count rises above the number. It fails again
when the count falls below the number, until someone lowers the number. The check records
both directions.
