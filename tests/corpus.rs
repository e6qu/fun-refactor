//! Translation against code somebody shipped.
//!
//! `tests/transpile.rs` and `tests/nextjs.rs` use fixtures, and a fixture is written by the
//! same person who writes the assertion, so they passed while `def create_user(*, session,
//! user_create)` produced `export function createUser(*. Unknown, …)`, a file TypeScript will
//! not parse.
//!
//! The corpus is four projects, vendored unmodified and pinned; see
//! `tests/corpus/PROVENANCE.md`. What is asserted here is deliberately not "the output equals
//! this string". That would freeze today's translation and break on every improvement. It is
//! the three properties that must hold for any translation to be worth reading:
//!
//! 1. **It parses as what it claims to be.** Anything else is a defect in this tool.
//! 2. **It adopts the target's conventions.** `user_create` is `userCreate` in TypeScript, and
//!    a file that says otherwise reads as converted and not written.
//! 3. **Nothing goes missing quietly.** Every construct that did not translate is in the output
//!    verbatim and counted.

use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;
use fun_refactor::transpile::{self, nextjs};
use std::path::{Path, PathBuf};

/// Copy the corpus into a temporary workspace, since translating writes beside the
/// source and the corpus must stay as vendored.
fn corpus(subdirectory: &str) -> (tempfile::TempDir, PathBuf) {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(subdirectory);
    let tmp = tempfile::tempdir().unwrap();
    copy(&from, tmp.path());
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn parses_as(language: Language, source: &str) -> bool {
    let parsers = Parsers::new();
    parsers
        .parse(language, source)
        .map(|parsed| !parsed.has_errors())
        .unwrap_or(false)
}

// ------------------------------------------------------- a real FastAPI backend

const FASTAPI_FILES: &[&str] = &["crud.py", "models.py", "security.py"];

#[test]
fn every_backend_file_translates_into_typescript_that_parses() {
    // The translator checks this itself and refuses to write output that does not parse. So a
    // failure here is `plan` returning an error, which is the defect report working, and still
    // a defect.
    for name in FASTAPI_FILES {
        let (_tmp, root) = corpus("fastapi");
        let plan = transpile::plan(&root.join(name), Language::TypeScript)
            .unwrap_or_else(|e| panic!("{name} -> typescript: {e}"));
        assert!(
            parses_as(Language::TypeScript, &plan.output),
            "{name} produced TypeScript that does not parse:\n{}",
            plan.output
        );
    }
}

#[test]
fn every_backend_file_translates_into_go_and_rust_that_parse() {
    // Two targets that share far less with Python than TypeScript does. They carry
    // more; what they must not do is emit something their own grammar rejects.
    for name in FASTAPI_FILES {
        for target in [Language::Go, Language::Rust] {
            let (_tmp, root) = corpus("fastapi");
            let plan = transpile::plan(&root.join(name), target)
                .unwrap_or_else(|e| panic!("{name} -> {target}: {e}"));
            assert!(
                parses_as(target, &plan.output),
                "{name} produced {target} that does not parse:\n{}",
                plan.output
            );
        }
    }
}

#[test]
fn a_python_signature_arrives_in_typescript_spelled_the_typescript_way() {
    let (_tmp, root) = corpus("fastapi");
    let plan = transpile::plan(&root.join("crud.py"), Language::TypeScript).unwrap();

    // `def create_user(*, session: Session, user_create: UserCreate) -> User`
    assert!(
        plan.output
            .contains("export function createUser(session: Session, userCreate: UserCreate): User"),
        "the signature did not carry with the target's conventions:\n{}",
        plan.output
    );
    // Locals too, not only the declaration, a file whose declarations are renamed and
    // whose bodies are not is worse than one that renames nothing.
    assert!(plan.output.contains("let extraData"), "{}", plan.output);
    assert!(plan.output.contains("dbUser: User"), "{}", plan.output);
}

#[test]
fn a_keyword_only_marker_is_reported_rather_than_written_as_a_parameter() {
    // `*` is a rule about the parameters around it. It is not a parameter. Written as one it
    // produced `createUser(*: unknown, …)`; dropped in silence the signature would
    // look carried when the way callers must invoke it had changed.
    let (_tmp, root) = corpus("fastapi");
    let plan = transpile::plan(&root.join("crud.py"), Language::TypeScript).unwrap();

    assert!(!plan.output.contains("*: unknown"), "{}", plan.output);
    assert!(
        plan.fidelity.signatures_with_changed_calls > 0,
        "the change of calling convention has to be counted: {:?}",
        plan.fidelity
    );
    assert!(
        plan.fidelity
            .notes
            .iter()
            .any(|n| n.contains("callers write the call differently")),
        "{:?}",
        plan.fidelity.notes
    );
}

#[test]
fn nineteen_typed_records_carry_as_typed_records() {
    // `models.py` is the case the typed-Python-to-typed-TypeScript work is for: every
    // record annotated, nothing inferred.
    let (_tmp, root) = corpus("fastapi");
    let plan = transpile::plan(&root.join("models.py"), Language::TypeScript).unwrap();

    assert!(
        plan.fidelity.records >= 15,
        "expected the file's records to carry, got {}",
        plan.fidelity.records
    );
    assert!(
        plan.output.contains("export interface UserCreate"),
        "{}",
        plan.output
    );
    assert!(
        plan.output.contains("fullName") || plan.output.contains("full_name: "),
        "a snake_case field has to become camelCase:\n{}",
        plan.output
    );
}

#[test]
fn a_foreign_library_type_is_never_renamed_to_suit_the_target() {
    // `Session`, `UserCreate` and the rest belong to sqlmodel and to the file's own
    // models. Re-casing a name this module does not declare renames somebody else's
    // API, which is the one thing a translation must not do.
    let (_tmp, root) = corpus("fastapi");
    let plan = transpile::plan(&root.join("security.py"), Language::TypeScript).unwrap();
    assert!(!plan.output.contains("session"), "{}", plan.output);
}

// --------------------------------------------------------- a real Next.js app

const ROUTES: &[&str] = &[
    "app/api/posts/route.ts",
    "app/api/posts/[postId]/route.ts",
    "app/api/webhooks/stripe/route.ts",
];

#[test]
fn every_route_becomes_a_fastapi_module_that_parses_as_python() {
    for route in ROUTES {
        let (_tmp, root) = corpus("nextjs");
        let plan = nextjs::plan(&root.join(route)).unwrap_or_else(|e| panic!("{route}: {e}"));
        assert!(
            parses_as(Language::Python, &plan.output),
            "{route} produced Python that does not parse:\n{}",
            plan.output
        );
        assert!(!plan.methods.is_empty(), "{route} found no HTTP method");
    }
}

#[test]
fn the_url_survives_the_crossing_with_the_targets_conventions() {
    let (_tmp, root) = corpus("nextjs");
    let plan = nextjs::plan(&root.join("app/api/posts/[postId]/route.ts")).unwrap();

    // `[postId]` is a placeholder name, not part of the URL, `/posts/{postId}` and
    // `/posts/{post_id}` serve exactly the same requests. So it takes Python's convention like
    // every other name FastAPI will see.
    assert_eq!(plan.route, "/posts/{post_id}");
    assert_eq!(plan.methods, vec!["DELETE", "PATCH"]);
    assert!(
        plan.output
            .contains("async def delete(post_id: str, req: Request):"),
        "{}",
        plan.output
    );
    assert_eq!(plan.destination.file_name().unwrap(), "posts_post_id.py");
}

#[test]
fn real_error_handling_carries_across() {
    // This route's handlers are one `try` each, and before `try` was in the IR the
    // whole body of every handler came out as a comment. Everything asserted here was
    // carried verbatim at some point in this file's history.
    let (_tmp, root) = corpus("nextjs");
    let plan = nextjs::plan(&root.join("app/api/posts/[postId]/route.ts")).unwrap();

    assert!(plan.output.contains("    try:"), "{}", plan.output);
    assert!(
        plan.output.contains("except Exception as error:"),
        "{}",
        plan.output
    );
    // `error instanceof z.ZodError` is `isinstance`, the same question, spelled as an
    // operator in one language and a builtin in the other.
    assert!(
        plan.output.contains("isinstance(error, z.ZodError)"),
        "{}",
        plan.output
    );
    // `new Response(null, { status: 403 })` is FastAPI's `Response(status_code=403)`,
    // and the body, where there is one, has to come with it.
    assert!(
        plan.output.contains("Response(status_code=403)"),
        "{}",
        plan.output
    );
    assert!(plan.output.contains("status_code=422"), "{}", plan.output);
    assert!(
        plan.output.contains("content="),
        "a response body must not be dropped:\n{}",
        plan.output
    );
    // A helper declared in the same file is renamed at its uses, not only where it is
    // declared, the handler bodies are written as their own modules and used to be
    // spelled without the rest of the file in view.
    assert!(
        plan.output
            .contains("await verify_current_user_has_access_to_post("),
        "{}",
        plan.output
    );
}

#[test]
fn a_comment_is_translated_rather_than_reported_as_a_failure() {
    // Every one of these languages has comments and only the marker differs. Reading
    // one as an untranslatable construct put ordinary prose under a "not translated"
    // marker and counted it among the real gaps.
    let (_tmp, root) = corpus("nextjs");
    let plan = nextjs::plan(&root.join("app/api/posts/[postId]/route.ts")).unwrap();

    assert!(
        plan.output.contains("# Validate the route params."),
        "{}",
        plan.output
    );
    assert!(
        !plan.output.contains("not translated: comment"),
        "{}",
        plan.output
    );
}

#[test]
fn what_does_not_translate_is_in_the_output_verbatim_and_counted() {
    // Destructuring used to be the example here, until it learned to lower. The
    // promises stand: what still carries is *there*, counted, and marked, and what
    // stopped carrying is translated, and never both at once.
    let (_tmp, root) = corpus("nextjs");
    let plan = nextjs::plan(&root.join("app/api/posts/[postId]/route.ts")).unwrap();

    assert!(plan.fidelity.carried_verbatim > 0);
    assert_eq!(
        plan.fidelity.carried_verbatim,
        plan.output.matches(transpile::MARKER).count(),
        "the count and the markers in the file have to agree:\n{}",
        plan.output
    );
    assert!(
        plan.output
            .contains("params = route_context_schema.parse(context).params"),
        "the destructuring stopped lowering:\n{}",
        plan.output
    );
}

#[test]
fn optional_chaining_is_never_written_away() {
    // `session?.user.id` is not `session.user.id`. No target here has optional
    // chaining, and the plain access compiles, runs, and throws where the original
    // returned undefined, a silent wrong answer, the one outcome worse than a gap.
    let (_tmp, root) = corpus("nextjs");
    let plan = nextjs::plan(&root.join("app/api/posts/[postId]/route.ts")).unwrap();

    assert!(
        !plan.output.contains("None.id"),
        "an optional chain collapsed into a null access:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("session?.user.id"),
        "it has to be carried, with the original:\n{}",
        plan.output
    );
}

#[test]
fn a_route_that_is_a_component_is_still_refused() {
    // The corpus has none, so this builds one beside it. The refusal is the load bearing half
    // of the feature and must not depend on a fixture directory.
    let (_tmp, root) = corpus("nextjs");
    let path = root.join("app/api/widget/route.tsx");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "export default function W() {\n  return <div/>;\n}\n",
    )
    .unwrap();

    let error = nextjs::plan(&path).expect_err("JSX is not an API route");
    assert!(error.to_string().contains("JSX"), "{error}");
}
