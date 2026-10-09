#!/usr/bin/env python3
"""Exercise real guide admission and body-review boundaries on tiny hosted fixtures."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("bodies", ROOT / "tools/guide-body-context.py")
bodies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bodies)
routes = bodies.routes


class BodyAdmission(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="fr-body-admission-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / "project"
        routes.prepare(self.root, routes.SOURCES[1])
        self.client = routes.RecordedClient(self.root, executable=os.environ.get(
            "FR_TEST_BINARY", str(ROOT / "target/debug/fr")))

    def goal(self, **changes):
        return routes.AgentGoal(**{
            "purpose": "change", "selector": routes.GoalSelector(name="normalize", scope="text.py"),
            "operation": routes.GoalOperation("source-body"),
            "constraints": routes.GoalConstraints(allow_source=True), "checks": ("behavior",),
            "delivery": routes.TaskDelivery(patch="artifacts/change.patch"), **changes})

    def assert_untouched(self):
        self.assertEqual(routes.source_state(self.root, routes.SOURCES[1]), routes.SOURCES[1]["files"])
        self.assertFalse((self.root / ".fr-history").exists())
        self.assertFalse((self.root / "artifacts/change.patch").exists())

    def test_clipped_diff_refuses_before_transaction(self):
        guide = self.client.guide(self.goal())
        action = guide.source_body_action({guide.target.handle: "return " + repr("x" * 6000) + "\n"})
        with self.assertRaises(routes.FrRuntimeError):
            self.client.review_guide(guide, action)
        self.assertEqual(len(self.client.events), 2)
        self.assert_untouched()

    def test_ambiguous_selection_cannot_prepare_body_action(self):
        (self.root / "other.py").write_text("def normalize(value):\n    return value\n")
        guide = self.client.guide(self.goal(selector=routes.GoalSelector(name="normalize")))
        self.assertEqual(guide.at("/state"), "needs-selection")
        with self.assertRaises(routes.FrRuntimeError):
            guide.source_body_action({"normalize": "return value\n"})
        self.assertEqual(len(self.client.events), 1)
        self.assert_untouched()

    def test_source_permission_is_required(self):
        guide = self.client.guide(self.goal(constraints=routes.GoalConstraints()))
        with self.assertRaises(routes.FrRuntimeError):
            guide.source_body_action({guide.target.handle: "return value\n"})
        self.assertEqual(len(self.client.events), 1)
        self.assert_untouched()


if __name__ == "__main__":
    unittest.main()
