#!/usr/bin/env python3
"""Exercise exact multi-body guide admission and one checked two-file delivery."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from fr_ir.guide import (AgentGoal, GoalConstraints, GoalLimits, GoalOperation, GoalSelector,
                         GuideFile, GuideInputs)
from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrRuntimeError


SOURCES = {
    "Layout.tsx": "export const Layout = () => { return <main>Body</main>; };\n",
    "Header.tsx": "export const Header = () => { return <header>Title</header>; };\n",
    "Other.tsx": "export const Other = () => { return <aside>Other</aside>; };\n",
}
BODIES = {
    "Layout.tsx": "{ return <main aria-label='Body'>Body</main>; }",
    "Header.tsx": "{ return <header aria-label='Title'>Title</header>; }",
    "Other.tsx": "{ return <aside aria-label='Other'>Other</aside>; }",
}


def git(root: Path, *arguments: str) -> None:
    subprocess.run(["git", *arguments], cwd=root, check=True, capture_output=True)


def main(binary: Path) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="fr-source-bodies-") as temporary:
        root = Path(temporary) / "project"
        root.mkdir()
        for name, source in SOURCES.items():
            (root / name).write_text(source)
        (root / ".fr").mkdir()
        (root / ".fr/checks.json").write_text(json.dumps({"schema": 1, "checks": [{
            "name": "syntax", "argv": ["python3", "-c", "print('source-stable')"],
            "cwd": ".", "timeout_seconds": 30, "covers": ["source-stable declared check"],
        }]}))
        (root / "artifacts").mkdir()
        git(root, "init", "-q")
        git(root, "add", "--all")
        git(root, "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid", "commit", "-qm", "initial")
        delivery = TaskDelivery(patch="artifacts/bodies.patch")
        goal = AgentGoal("change", selector=GoalSelector(name="Layout", scope="Layout.tsx", language="tsx"),
                         operation=GoalOperation("source-bodies", {"additional": [{
                             "name": "Header", "scope": "Header.tsx", "language": "tsx",
                         }]}), constraints=GoalConstraints(allow_source=True), checks=("syntax",),
                         delivery=delivery, context=GoalLimits(packet_limit=65536))
        client = FrClient(str(root), executable=str(binary))
        guide = client.guide(goal)
        targets = guide.at("/targets")
        assert guide.at("/route/id") == "source-bodies" and len(targets) == 2
        assert [item["path"] for item in targets] == ["Layout.tsx", "Header.tsx"]
        for index in (0, 1):
            reveal = client.follow_guide(guide.actions()[index])
            assert reveal.at("/source/returned_bytes") < 4096
        inputs = Path(temporary) / "inputs"
        inputs.mkdir()
        operations = []
        for index, name in enumerate(("Layout.tsx", "Header.tsx")):
            path = inputs / f"body-{index}.txt"
            path.write_text(BODIES[name])
            operations.append({"op": "replace-body", "handle": targets[index]["handle"], "from": str(path)})
        preview = client.follow_guide(guide.actions()[2], GuideInputs({
            "input-file": GuideFile("batch.json", json.dumps({"operations": operations})),
        }))
        assert preview.at("/files_changed") == 2 and preview.at("/applied") is False
        assert all((root / name).read_text() == source for name, source in SOURCES.items())

        def change(names: tuple[str, ...]) -> TaggedIntentAction:
            handles = {item["path"]: item["handle"] for item in targets}
            other = client.guide(AgentGoal("understand", selector=GoalSelector(name="Other", scope="Other.tsx")))
            handles["Other.tsx"] = other.at("/target/handle")
            targets_ir = [TaskTarget(name.removesuffix(".tsx").lower(), handles[name], "replace-body",
                                     fragment=BODIES[name]) for name in names]
            manifest = TaskChange([], targets_ir, {"files-changed": len(names), "edits": len(names),
                                                   "paths-changed": list(names)}, ["syntax"], delivery)
            return TaggedIntentAction(TaskChangeOperation(manifest), diff_bytes=65536, report_bytes=65536)

        for names in (("Layout.tsx",), ("Layout.tsx", "Header.tsx", "Other.tsx")):
            try:
                client.review_guide(guide, change(names))
            except FrRuntimeError as error:
                assert "differs from its navigator goal" in str(error), str(error)
            else:
                raise AssertionError(f"guide accepted wrong targets: {names}")
        review = client.review_guide(guide, change(("Layout.tsx", "Header.tsx")))
        assert review.at("/ready") is True
        result = client.execute_guide(review)
        assert result.at("/passed") is True
        assert [row["status"] for row in result.at("/workflow/stages")] == ["passed"] * 8
        assert (root / "artifacts/bodies.patch").is_file()
        return {"passed": True, "targets": [item["path"] for item in targets],
                "wrong_target_sets_refused": 2, "delivery_stages": 8}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(main(args.fr.resolve()), indent=2))
