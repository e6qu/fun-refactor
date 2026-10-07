"""Prepare the complete packaging review without model calls or candidate execution."""
import base64
import gzip
import hashlib
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
PACK = ROOT / "tests/agent-eval/opencode/candidates"
sys.path.insert(0, str(ROOT / "tools"))

from agent_eval import change_controls, rehearsal_evidence, source_packets, source_reviews
from agent_eval.study import digest, encode, load, require


def prepare():
    task = next(t for t in load(PACK / "manifest.json")["tasks"] if t["id"] == "packaging-prerelease")
    baseline = rehearsal_evidence.source_bundle(task, PACK)
    control = next(c for c in load(PACK / "controls.json")[task["id"]] if c["id"] == "reference")
    reference = change_controls.apply(baseline, control)
    files = {**reference, **{"baseline/" + path: row for path, row in baseline.items()}}
    comparison = source_packets.compare(files, [{"before": "baseline/" + p, "after": p}
        for p in ("src/packaging/specifiers.py", "src/packaging/version.py")])
    grader = load(PACK / task["grader"])
    public = load(PACK / "public-checks.json")[task["id"]]
    extra = {"requirement.txt": task["requirement"], "grader.py": grader["command"][-1],
             "cases.json": encode(grader["cases"]).decode(), "public-check.py": public["command"][-1],
             "reference.json": encode(control).decode(), "file-comparison.json": encode(comparison).decode()}
    for name, text in extra.items():
        files["review/" + name] = {"data": base64.b64encode(text.encode()).decode(), "executable": False}
    selections = []
    for path in ("review/requirement.txt", "review/file-comparison.json", "review/grader.py"):
        raw = base64.b64decode(files[path]["data"], validate=True)
        selections.append({"path": path, "sha256": hashlib.sha256(raw).hexdigest(), "start": 0, "end": len(raw)})
    # Reuse a fr-selected declaration only after checking the complete source identity.
    previous = load(HERE.parent / "2026-10-06-configured/design.json")
    old = next(q for q in previous["questions"] if q["id"] == "packaging-one-shot-duplicates")
    selected = [s for s in old["selections"] if s["path"] == "src/packaging/specifiers.py"]
    require(len(selected) == 1, "expected one prior fr declaration selection")
    selections.extend(selected)
    source_packets.build(files, selections)
    source_packets.check_comparison(files, comparison)
    archive = gzip.compress(encode({task["id"]: files}), mtime=0)
    require(len(archive) <= 1024**2, "review archive exceeds budget")
    question = (
        "Review the complete packaging task, its proposed reference and all eight private grader cases. "
        "Source under src/ is the author's proposed reference, not a proven answer; baseline/src/ is original. "
        "The file comparison establishes byte identities only. The complete requirement and grader are supplied. "
        "Review intersection-wide fallback, final/postrelease suppression, constructor and call overrides, "
        "prerelease bounds and exclusions, empty sets, Version objects, per-occurrence identity, order, duplicates, "
        "one-shot consumption, and preservation of contains and individual Specifier behavior. "
        "Additional source, review/reference.json and review/public-check.py are available through tools. "
        "Find at most one concrete reference violation, incorrect assertion or plausible wrong repair accepted "
        "by the current grader. Supply a distinguishing input, expected result and source citations. "
        "Do not limit the review to the earlier duplicate-output example. "
        "In limitations, name any requirement areas you did not inspect. "
        "An empty finding list is not whole-task acceptance. Do not execute or edit code."
    )
    design = {"schema": "fr-terminal-review-design-1",
              "source_archive": {"path": str((HERE / "inputs.json.gz").relative_to(ROOT)),
                                 "sha256": hashlib.sha256(archive).hexdigest()},
              "models": previous["models"], "questions": [{"id": "packaging-whole-task",
                  "source_task": task["id"], "question": question, "selections": selections,
                  "comparison": "review/file-comparison.json"}]}
    identities = {"schema": "fr-packaging-review-inputs-1", "preparation_sha256": source_reviews.identity(__file__),
                  "task_sha256": digest(task), "baseline_sha256": digest(baseline),
                  "reference_sha256": digest(reference), "control_sha256": digest(control),
                  "grader_sha256": digest(grader), "public_check_sha256": digest(public),
                  "cases": [c["id"] for c in grader["cases"]], "design_sha256": digest(design),
                  "execution_frozen": False, "whole_task_accepted": False}
    return {"inputs.json.gz": archive, "design.json": encode(design), "identities.json": encode(identities)}


def main():
    require(sys.argv[1:] in (["write"], ["check"]), "choose write or check")
    artifacts = prepare()
    for name, raw in artifacts.items():
        path = HERE / name
        if sys.argv[1] == "write":
            with path.open("xb") as stream:
                stream.write(raw)
        else:
            require(path.read_bytes() == raw, "review inputs changed: " + name)
    print("Complete packaging source bundle checked; two reviews remain unfrozen and unstarted.")


if __name__ == "__main__":
    main()
