//! Markdown renders to the HTML it describes, one way.

use fun_refactor::lang::Language;
use fun_refactor::translate;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn rendered(source: &str) -> String {
    let (_tmp, root) = workspace(&[("doc.md", source)]);
    let plan = translate::plan(&root.join("doc.md"), Language::Html).expect("a render");
    let (_, edits) = plan.edits.iter().next().expect("one edit");
    edits[0].replacement.clone()
}

#[test]
fn the_subset_renders_as_the_html_it_describes() {
    let html = rendered(
        "# Title\n\nSome *em* and **strong** with `code` and a [link](https://x.example/a).\n\n\
         - one\n- two\n\n1. first\n2. second\n\n> quoted\n\n```rust\nlet x = 1;\n```\n\n---\n",
    );
    for expected in [
        "<h1>Title</h1>",
        "<em>em</em>",
        "<strong>strong</strong>",
        "<code>code</code>",
        "<a href=\"https://x.example/a\">link</a>",
        "<ul>\n<li>one</li>",
        "<ol>\n<li>first</li>",
        "<blockquote>\n<p>quoted</p>\n</blockquote>",
        "<pre><code class=\"language-rust\">let x = 1;\n</code></pre>",
        "<hr />",
    ] {
        assert!(html.contains(expected), "missing `{expected}`:\n{html}");
    }
}

#[test]
fn text_is_escaped_and_raw_html_passes_through() {
    let html = rendered("A paragraph with <b>raw</b> tags and 1 < 2 & so on.\n");
    assert!(
        html.contains("1 &lt; 2 &amp; so on."),
        "text escapes:\n{html}"
    );
    let block = rendered("<div class=\"keep\">\nkept as written\n</div>\n");
    assert!(
        block.contains("<div class=\"keep\">"),
        "an html block crosses as itself:\n{block}"
    );
}

#[test]
fn the_reverse_direction_refuses_with_the_reason() {
    let (_tmp, root) = workspace(&[("page.html", "<p>hello</p>\n")]);
    let refused = translate::plan(&root.join("page.html"), Language::Markdown);
    assert!(refused.is_err(), "html does not become markdown");
}

#[test]
fn what_the_render_does_not_know_is_never_dropped() {
    // A reference-style link needs its definition resolved, which the subset
    // does not do; the text still reaches the output, escaped and marked.
    let html = rendered("See [the spec][1].\n\n[1]: https://example.com/spec\n");
    assert!(
        html.contains("the spec"),
        "the words are in the output:\n{html}"
    );
}
