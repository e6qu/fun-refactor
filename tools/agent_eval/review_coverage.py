"""Report assigned review coverage without turning submissions into acceptance."""
import copy
import re

from . import terminal_reviews as review
from .study import require


def checked(value, questions):
    require(isinstance(value, list) and 1 <= len(value) <= 64, "invalid review coverage")
    identifiers, assigned = set(), set()
    for row in value:
        require(isinstance(row, dict) and set(row) == {"id", "description", "question"}, "invalid coverage assignment")
        name = row["id"]
        require(isinstance(name, str) and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", name)
                and name not in identifiers, "invalid or duplicate coverage identity")
        require(isinstance(row["description"], str) and row["description"].strip()
                and len(row["description"].encode()) <= 512, "invalid coverage description")
        require(isinstance(row["question"], str) and row["question"] in questions, "unknown coverage question")
        identifiers.add(name)
        assigned.add(row["question"])
    require(assigned == set(questions), "question has no assigned coverage")
    return copy.deepcopy(value)


def report(frozen, snapshots, output):
    result = review.report(frozen, snapshots, output)
    plan = frozen["plan"]
    assignments = checked(plan["provenance"].get("coverage"), [t["id"] for t in plan["tasks"]])
    questions = []
    for task in plan["tasks"]:
        attempts = []
        for row in result["attempts"]:
            if row["cell"]["task"] != task["id"]:
                continue
            model = plan["models"][row["cell"]["model"]]
            accepted = row["status"] == "completed"
            attempts.append({"cell": row["cell"]["id"], "model": {k: model[k] for k in ("providerID", "modelID")},
                "status": row["status"], "failure": row.get("failure"),
                "findings": row["audit"]["review"]["findings"] if accepted else None,
                "limitations": row["audit"]["review"]["limitations"] if accepted else None})
        questions.append({"id": task["id"], "assignments": [a for a in assignments if a["question"] == task["id"]],
            "all_submitted": all(a["status"] == "completed" for a in attempts), "attempts": attempts})
    return {"schema": "fr-review-coverage-1", "plan_sha256": frozen["sha256"], "questions": questions,
        "assigned_requirements": len(assignments),
        "requirements_with_complete_submissions": sum(len(q["assignments"]) for q in questions if q["all_submitted"]),
        "completed": result["completed"], "failed": result["failed"], "not_started": result["not_started"],
        "all_submitted": all(q["all_submitted"] for q in questions), "claims_verified": False, "task_accepted": False,
        "scope": "Assignments and submission status only. Inspect findings and limitations, verify claims, and assess each requirement before task acceptance."}
