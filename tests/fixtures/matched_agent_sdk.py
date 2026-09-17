import json
import sys

from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector, GuideFile, GuideInputs
from fr_ir.runtime import FrClient

client = FrClient(sys.argv[1], executable=sys.argv[2])
client.complete_guide(AgentGoal("understand", selector=GoalSelector(name="calculate")))
client.complete_guide(AgentGoal("trace", selector=GoalSelector(name="calculate")))
client.complete_guide(AgentGoal(
    "change", selector=GoalSelector(name="calculate"),
    operation=GoalOperation("capability", {
        "capability": "rename", "parameters": {"new_name": "compute"},
    }),
))
recipe = """schema 1
recipe rename-calculate {
  rename to "compute" where name="calculate"
  expect matched = 1
  expect changed = 1 files
  expect refusals = 0
}
"""
client.complete_guide(AgentGoal(
    "change", selector=GoalSelector(name="calculate"),
    operation=GoalOperation("recipe", {"verb": "rename"}),
), {0: GuideInputs({"recipe-file": GuideFile("rename.recipe", recipe)})})
client.complete_guide(AgentGoal(
    "change", selector=GoalSelector(name="calculate"),
    operation=GoalOperation("semantic-scalar", {
        "operation": "set-int", "from": "7", "to": "9",
    }),
))
migration_goal = AgentGoal(
    "migrate", selector=GoalSelector(path="app/api/signals/route.ts"),
    operation=GoalOperation("framework-migration", {"to": "fastapi"}),
)
migration = client.guide(migration_goal)
feature = migration.at("/route/evidence/compatible_features")[0]
client.follow_guide(migration.actions()[0])
client.follow_guide(migration.actions()[1], GuideInputs({
    "feature-id": feature, "destination": "converted/signals.py",
}))
proof_goal = AgentGoal(
    "prove", selector=GoalSelector(path="specs/FrSpecs/SrcLibRsKeep.lean"),
    operation=GoalOperation("proof", {"obligation": "keepModel_identity"}),
)
proof_file = GuideFile("proof.lean", "rfl\n")
client.complete_guide(proof_goal, {
    1: GuideInputs({"tactics-file": proof_file}),
    2: GuideInputs({"tactics-file": proof_file}),
})
print(json.dumps({
    "schema": "fr-matched-outcomes-1",
    "workflows": [
        {"id": "understanding", "path": "src/lib.rs", "name": "calculate",
         "kind": "function", "language": "rust"},
        {"id": "tracing", "path": "src/lib.rs", "name": "calculate",
         "direct_callers": 0},
        {"id": "direct-change", "path": "src/lib.rs", "from": "calculate",
         "to": "compute", "changed_files": 1},
        {"id": "recipe", "verb": "rename", "from": "calculate", "to": "compute",
         "matched": 1, "changed_files": 1, "refusals": 0},
        {"id": "semantic-edit", "path": "src/lib.rs", "operation": "set-int",
         "from": "7", "to": "9", "changed_files": 1},
        {"id": "framework-migration", "source": "app/api/signals/route.ts",
         "target": "fastapi", "destination": "converted/signals.py", "method": "GET",
         "path": "/api/signals", "response": {"healthy": True}},
        {"id": "proof", "path": "specs/FrSpecs/SrcLibRsKeep.lean",
         "obligation": "keepModel_identity", "tactics": ["rfl"], "accepted": True},
    ],
}))
