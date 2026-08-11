#!/usr/bin/env python3
"""Count the writing habits that this project has decided against.

The rules come from ASD-STE100, Simplified Technical English, which aerospace
maintenance manuals use. `docs/style.md` says which of its rules apply here and why.

The counts run against a budget in `tools/PROSE-DEBT`. A count above its budget fails.
A count below it fails too, with a message that says to lower the budget: a number that
only ever drifts down without being written down stops measuring anything.

Run `--report` to see the budget file this run would write.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUDGET = ROOT / "tools" / "PROSE-DEBT"

DOCS = [
    # `docs/style.md` is absent on purpose: it quotes the habits it forbids.
    "README.md", "PLAN.md", "BUGS.md", "TUTORIAL.md", "RECIPES.md",
    "API_CONTRACTS.md", "CROSS_LANGUAGE.md", "EXAMPLES.md", "RESEARCH.md",
    "docs/terminology.md",
]

# Each rule is a name, a pattern and one line that says what to write instead.
RULES = [
    ("em-dash", r"—",
     "Use a full stop, a comma or a colon."),
    # Only the rhetorical shapes. "bind to the inner `let x`, not the outer one" is a
    # real contrast and carries real information, so the pattern asks for a copula:
    # assert a thing, then negate an alternative that was never a candidate.
    ("false-comparison",
     r"\b(is|are|was|were)\s+[^.\n]{1,40}?,\s+not\s+|"
     r"\bis not [^.\n]{1,50}?[,;]\s*(it|they|this|that)\s+(is|are|was)\b|"
     r"\bnot [^.\n]{1,40}? but\b|\brather than\b",
     "Write the positive statement on its own."),
    ("filler", r"\b(simply|merely|exactly|actually|of course|in fact|it is worth noting)\b",
     "Delete the word. It adds nothing."),
    ("self-reference", r"\b(that is (why|how|what)|which is (why|how|what)|is what (makes|the))\b",
     "State the fact. Do not point at it."),
    ("long-sentence", None,
     "Keep sentences to 25 words or fewer."),
]

MAX_WORDS = 25


def comments_of(path: Path) -> str:
    """The comment text of a Rust file, with the markers removed."""
    out = []
    for line in path.read_text().split("\n"):
        stripped = line.strip()
        if stripped.startswith("//"):
            out.append(re.sub(r"^/{2,3}!?\s?", "", stripped))
    return "\n".join(out)


def messages_of(path: Path) -> str:
    """The sentences a user reads: string literals long enough to be prose."""
    text = path.read_text()
    out = []
    for match in re.finditer(r'"((?:[^"\\]|\\.)*)"', text, re.S):
        raw = re.sub(r"\\\s*\n\s*", " ", match.group(1))
        if len(raw) > 40 and " " in raw and not raw.startswith(("@", "(", "http")):
            out.append(raw)
    return "\n".join(out)


def sentences(text: str):
    for part in re.split(r"(?<=[.!?])\s+|\n\n", text):
        part = part.strip()
        if part:
            yield part


def count(text: str) -> dict:
    found = {name: 0 for name, _, _ in RULES}
    for name, pattern, _ in RULES:
        if pattern:
            found[name] = len(re.findall(pattern, text, re.I))
    for sentence in sentences(text):
        # Tables, code and long identifiers are not prose.
        if "|" in sentence or "`" in sentence and len(sentence.split()) > 40:
            continue
        if len(sentence.split()) > MAX_WORDS:
            found["long-sentence"] += 1
    return found


def gather() -> dict:
    totals = {name: 0 for name, _, _ in RULES}
    for path in sorted(ROOT.glob("src/**/*.rs")) + sorted(ROOT.glob("tests/**/*.rs")):
        for text in (comments_of(path), messages_of(path)):
            for name, n in count(text).items():
                totals[name] += n
    for name in DOCS:
        path = ROOT / name
        if path.exists():
            for key, n in count(path.read_text()).items():
                totals[key] += n
    return totals


def read_budget() -> dict:
    if not BUDGET.exists():
        return {}
    budget = {}
    for line in BUDGET.read_text().split("\n"):
        line = line.split("#")[0].strip()
        if line:
            name, value = line.split()
            budget[name] = int(value)
    return budget


def main() -> int:
    totals = gather()

    if "--report" in sys.argv:
        for name, _, _ in RULES:
            print(f"{name} {totals[name]}")
        return 0

    budget = read_budget()
    over, under = [], []
    for name, _, advice in RULES:
        allowed = budget.get(name)
        if allowed is None:
            over.append(f"{name}: {totals[name]} found and no budget set")
        elif totals[name] > allowed:
            over.append(f"{name}: {totals[name]}, budget {allowed}. {advice}")
        elif totals[name] < allowed:
            under.append(f"{name}: {totals[name]}, budget {allowed}")

    for name, _, _ in RULES:
        print(f"  {totals[name]:6}  {name}")

    if over:
        print("\nprose: these went up:")
        for line in over:
            print(f"  {line}")
        return 1
    if under:
        print("\nprose: these went down. Lower the budget in tools/PROSE-DEBT:")
        for line in under:
            print(f"  {line}")
        return 1
    print("\nprose: every count is at its budget.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
