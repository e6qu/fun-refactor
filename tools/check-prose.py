#!/usr/bin/env python3
"""Count the writing habits that this project has decided against.

The rules come from ASD-STE100, Simplified Technical English, which aerospace
maintenance manuals use. `docs/style.md` says which of its rules apply here and why.

The counts run against a budget in `tools/PROSE-DEBT`. A count above its budget fails.
A count below it fails too, with a message that says to lower the budget: a number that
only ever drifts down without being written down stops measuring anything.

Run `--report` to see the budget file this run would write.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
BUDGET = ROOT / "tools" / "PROSE-DEBT"

DOCS = [
    # `docs/style.md` is absent on purpose: it quotes the habits it forbids.
    "README.md", "PLAN.md", "BUGS.md", "TUTORIAL.md", "RECIPES.md",
    "API_CONTRACTS.md", "CROSS_LANGUAGE.md", "EXAMPLES.md", "RESEARCH.md",
    "CLI.md", "IR.md",
    "docs/terminology.md", "docs/lean-specs.md",
]

# Each rule is a name, a pattern and one line that says what to write instead.
RULES = [
    ("em-dash", r"—",
     "Use a full stop, a comma or a colon."),
    # Negate, then restate. "A dispatch edge is a candidate, not a proven call" tells a
    # reader what a thing is not and leaves them to work out what it is.
    #
    # Twice narrowed, both times after reading what it caught. "the guard was
    # file-scoped, not scope-scoped" and "compares structure, not text" name the
    # thing a reader would otherwise assume, and that is the most precise sentence
    # available. A rule against every negation deletes those. This asks for the shape
    # where the negation carries the weight and the positive claim arrives second, or
    # never.
    ("false-comparison",
     r"\bis not [^.\n]{1,50}?[,;]\s*(it|they|this|that)\s+(is|are|was)\b|"
     r"\bnot [^.\n]{1,40}? but\b",
     "Write the positive claim first, and let the negation follow it or go."),
    # "exactly one" and "rewrites exactly the bytes of a name span" are precise, so the
    # pattern asks for the emphatic form: `exactly` in front of a demonstrative or a
    # wh-word, where it adds heat and no information.
    ("filler",
     r"\b(simply|merely|actually|of course|in fact|it is worth noting)\b|"
     r"\bexactly\s+(what|how|this|that|why|when|where|like|as)\b",
     "Delete the word. It adds nothing."),
    # "which is what `str | None = None` says" identifies a thing, and the clause is the
    # shortest way to say it. What this asks for is the sentence that points back at the
    # text instead of carrying it: "that is why", "is what makes".
    ("self-reference", r"\b(that is (why|how|what)|is what (makes|the))\b",
     "State the fact. Do not point at it."),
    # Passive voice hides who acts. `docs/style.md` asks for the active,
    # imperative form: "Read both halves", never "both halves are read".
    #
    # A form of `be` in front of a past participle. The participle list is
    # explicit rather than a `-ed` pattern, because `-ed` also ends the past
    # tense of every regular verb, and "the walker yielded" is not passive.
    #
    # The list reached zero once while 365 constructions stood in the tree, on
    # verbs it had never named. A word joins it when it reads as a verb here, so
    # the predicate adjectives stay out: "is nested", "is guarded", "is required"
    # and "is unused" describe a state and name no actor to promote.
    ("passive-voice",
     r"\b(is|are|was|were|be|been|being|gets|got)\s+"
     r"(not\s+)?(?:already\s+|then\s+|also\s+|still\s+|never\s+|only\s+)?"
     r"(written|read|given|taken|made|held|kept|carried|counted|dropped|added|"
     r"removed|deleted|renamed|parsed|indexed|scanned|reported|returned|called|"
     r"passed|spelled|named|built|checked|compared|sorted|resolved|skipped|"
     r"treated|used|found|left|set|seen|shown|split|stored|applied|emitted|"
     r"produced|replaced|rewritten|translated|declared|derived|computed|"
     r"generated|measured|pinned|driven|chosen|picked|wrapped|bound|"
     r"moved|located|extracted|placed|inlined|verified|repointed|traced|ranked|"
     r"spliced|handed|substituted|redeclared|updated|qualified|renumbered|"
     r"expressed|decided|masked|supplied|included|recorded|offered|listed|"
     r"executed|assigned|anchored|addressed|tested|refused|referenced|"
     r"implemented|contained|changed|evaluated|followed|reached|keyed|quoted|"
     r"inserted|appended|attached|collected|converted|copied|created|discarded|"
     r"filtered|flattened|gathered|grouped|hoisted|ignored|inspected|joined|"
     r"lifted|lowered|marked|merged|narrowed|omitted|ordered|overwritten|"
     r"printed|promoted|published|raised|rejected|rendered|repeated|reused|"
     r"routed|selected|separated|shipped|stripped|swapped|tagged|tracked|"
     r"transformed|truncated|unwrapped|validated|walked|widened|yielded)\b",
     "Say who acts: \"Read both halves\", not \"both halves are read\"."),
    ("long-sentence", None,
     "Keep sentences to 25 words or fewer."),
    # Comment volume. Every count above says how a sentence is written; this one
    # says how many there should be. `docs/style.md` asks for none by default.
    ("comment-line", None,
     "Delete the comment, or cut it to the one line that carries the constraint."),
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
    """The sentences a user reads: string literals long enough to be prose.

    Escape sequences are decoded before counting. A literal ending `.\\n` ends a
    sentence in what the user sees, and counting it as mid-sentence glued every
    such message to the next string in the file.

    Char literals go first. `'"'` is a character, not a string delimiter, and
    reading it as one flipped the lexer's parity, so whole spans of code between
    two such characters counted as prose.
    """
    text = path.read_text()
    text = re.sub(r"'(?:[^'\\\n]|\\(?:.|u\{[0-9a-fA-F]+\}))'", "' '", text)
    out = []
    for match in re.finditer(r'"((?:[^"\\]|\\.)*)"', text, re.S):
        raw = re.sub(r"\\\s*\n\s*", " ", match.group(1))
        raw = raw.replace("\\n", "\n").replace("\\t", " ").replace('\\"', '"')
        # A literal holding a shell expansion is a program this tool writes,
        # not a sentence anybody reads. `fr completions` emits one line at a
        # time, and the lines glued together counted as a 74-word sentence.
        code = "${" in raw or "$(" in raw or "=>" in raw or "fn " in raw
        if len(raw) > 40 and " " in raw and not code and not raw.startswith(("@", "(", "http")):
            out.append(raw)
    return "\n".join(out)


def sentences(text: str):
    for part in re.split(r"(?<=[.!?])\s+|\n\n", text):
        part = part.strip()
        if part:
            yield part


def count(text: str) -> dict:
    # A Markdown table row is data. Its cells hold `—` for "not applicable", and reading
    # that as prose counted 60 of them in one table.
    text = "\n".join(l for l in text.split("\n") if not l.lstrip().startswith("|"))
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


def comment_lines(path: Path) -> int:
    """Lines of `//` comment in a Rust file.

    Counted rather than read, because the rule is about how many there are.
    """
    return sum(1 for line in path.read_text().split("\n") if line.strip().startswith("//"))


def gather() -> dict:
    totals = {name: 0 for name, _, _ in RULES}
    for path in sorted(ROOT.glob("src/**/*.rs")) + sorted(ROOT.glob("tests/**/*.rs")):
        totals["comment-line"] += comment_lines(path)
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
