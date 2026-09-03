#!/usr/bin/env python3
"""Cut Rust comments to the one line that carries the constraint.

`docs/style.md` asks for no comment by default, one line where a reader would
otherwise break something, and two where the constraint needs two. It forbids
narrative, justification and restatement.

Run `--report` to see what would change. Run `--write` to change it.

The scanner tracks strings so a fixture holding a line that starts with `//`
survives. `write.rs` holds several.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent

# A comment whose first sentence opens this way carries no constraint.
RESTATEMENT = re.compile(
    r"^(this|the|a|an)?\s*(function|method|helper|struct|enum|field|test|module|"
    r"pass|loop|block|match|closure|call)?\s*(returns?|builds?|holds?|walks?|"
    r"collects?|maps?|wraps?|calls?|runs?|does|makes?|takes?|gives?)\b",
    re.I,
)

# Narrative about what happened before, which `BUGS.md` and the git log hold.
HISTORY = re.compile(
    r"\b(used to|once|previously|earlier|the first (attempt|version|run)|"
    r"before this|was written|had been|went stale|drifted|turned out|"
    r"this was|it was tried|at first|originally|B\d{2,})\b",
    re.I,
)


def spans(text: str):
    """Yield (kind, start, end) over `text`: 'code', 'comment' or 'string'."""
    i, n = 0, len(text)
    out = []
    while i < n:
        c = text[i]
        # Raw string: r"…", r#"…"#, r##"…"##
        if c == "r" and i + 1 < n and text[i + 1] in '"#':
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                close = '"' + "#" * hashes
                end = text.find(close, j + 1)
                end = n if end < 0 else end + len(close)
                out.append(("string", i, end))
                i = end
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(("string", i, j))
            i = j
            continue
        if c == "'":
            # A lifetime, not a character literal, when no closing quote is near.
            m = re.match(r"'(?:\\.|[^'\\])'", text[i:])
            if m:
                out.append(("string", i, i + m.end()))
                i += m.end()
                continue
            i += 1
            continue
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            out.append(("comment", i, end))
            i = end
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i)
            end = n if end < 0 else end + 2
            out.append(("comment", i, end))
            i = end
            continue
        i += 1
    return out


def comment_line_numbers(text: str) -> set:
    """Lines that are wholly a `//` comment, strings excluded."""
    starts = [0]
    for i, c in enumerate(text):
        if c == "\n":
            starts.append(i + 1)
    real = set()
    for kind, a, _ in spans(text):
        if kind != "comment":
            continue
        line = len([s for s in starts if s <= a]) - 1
        before = text[starts[line] : a]
        if before.strip() == "":
            real.add(line)
    return real


def blocks(lines, commented):
    """Consecutive comment lines, as (first, last) inclusive."""
    out = []
    run = []
    for i in range(len(lines)):
        if i in commented:
            run.append(i)
        elif run:
            out.append((run[0], run[-1]))
            run = []
    if run:
        out.append((run[0], run[-1]))
    return out


def body(lines, first, last):
    """The block's text, markers removed, and its marker and indent."""
    marker = "//"
    for prefix in ("///", "//!"):
        if lines[first].strip().startswith(prefix):
            marker = prefix
            break
    indent = lines[first][: len(lines[first]) - len(lines[first].lstrip())]
    out = []
    for line in lines[first : last + 1]:
        stripped = line.strip()
        for prefix in ("///", "//!", "//"):
            if stripped.startswith(prefix):
                stripped = stripped[len(prefix) :]
                break
        out.append(stripped.strip())
    return " ".join(x for x in out if x).strip(), marker, indent


def first_sentence(text: str) -> str:
    """The leading sentence, with its full stop."""
    m = re.search(r"(?<=[.!?])(\s|$)", text)
    return text[: m.start()].strip() if m else text.strip()


def rewrap(sentence: str, marker: str, indent: str, width: int = 96):
    """The sentence as comment lines, wrapped to the file's width."""
    limit = width - len(indent) - len(marker) - 1
    out, line = [], ""
    for word in sentence.split():
        if line and len(line) + 1 + len(word) > limit:
            out.append(f"{indent}{marker} {line}")
            line = word
        else:
            line = f"{line} {word}".strip()
    if line:
        out.append(f"{indent}{marker} {line}")
    return out


def trim(path: Path):
    """The file's new lines, and what changed."""
    text = path.read_text()
    lines = text.split("\n")
    commented = comment_line_numbers(text)
    changes = []
    replacements = {}
    for first, last in blocks(lines, commented):
        content, marker, indent = body(lines, first, last)
        if not content:
            continue
        # An attribute-like or lint-directive comment carries a rule, not prose.
        if content.startswith(("cSpell", "clippy", "rustfmt", "SAFETY")):
            continue
        lead = first_sentence(content)
        rest = content[len(lead) :].strip()
        drop = marker == "//" and (RESTATEMENT.match(lead) or HISTORY.search(lead))
        if drop:
            replacements[(first, last)] = []
            changes.append((first + 1, content[:70], "delete"))
        elif rest or len(lines[first : last + 1]) > 2:
            replacements[(first, last)] = rewrap(lead, marker, indent)
            if rest:
                changes.append((first + 1, rest[:70], "cut"))
    if not replacements:
        return None, []
    out = []
    skip_to = -1
    for i, line in enumerate(lines):
        if i <= skip_to:
            continue
        hit = next((k for k in replacements if k[0] == i), None)
        if hit:
            out.extend(replacements[hit])
            skip_to = hit[1]
            continue
        out.append(line)
    return "\n".join(out), changes


def main() -> int:
    paths = sorted(ROOT.glob("src/**/*.rs")) + sorted(ROOT.glob("tests/**/*.rs"))
    if len(sys.argv) > 2:
        paths = [ROOT / p for p in sys.argv[2:]]
    write = "--write" in sys.argv
    removed = 0
    for path in paths:
        new, changes = trim(path)
        if new is None:
            continue
        before = len(path.read_text().split("\n"))
        after = len(new.split("\n"))
        removed += before - after
        if write:
            path.write_text(new)
        else:
            print(f"{path.relative_to(ROOT)}: -{before - after} line(s)")
    print(f"{'removed' if write else 'would remove'} {removed} line(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
