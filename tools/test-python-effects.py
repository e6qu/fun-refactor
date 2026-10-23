#!/usr/bin/env python3
"""Reject effect explanations that hide the runtime/model distinction."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("python_effects", Path(__file__).with_name("python-effects.py"))
effects = importlib.util.module_from_spec(spec)
spec.loader.exec_module(effects)


class Effects(unittest.TestCase):
    def fixture(self):
        example, = [row for row in effects.EXAMPLES if row["id"] == "implicit-stop"]
        program = effects.source(example, False)
        start = program.encode().index(b"1 // 0")
        report = {"complete": True, "witnesses": [{}], "runtime_effects": {
            "exception_safety_established": False, "shared_mutable_values_supported": False},
            "effect_limits": [{"kind": "implicit-exception", "occurrence": {"path": "subject.py",
                "location": {"span": {"start": start, "end": start + 6},
                    "range": {"start": {"line": 4, "col": 5}, "end": {"line": 4, "col": 11}}}}}]}
        execution = [{"sent": [], "raised": "ZeroDivisionError"}] * 2
        return example, program, report, execution

    def test_unexecutable_witness_requires_both_a_location_and_an_unchecked_runtime_scope(self):
        example, program, report, execution = self.fixture()
        effects.validate(example, report, execution, program, "subject.py")
        for change in ("location", "scope"):
            altered = copy.deepcopy(report)
            if change == "location":
                altered["effect_limits"][0]["occurrence"]["location"]["span"] = {"start": 0, "end": 6}
            else:
                altered["runtime_effects"]["exception_safety_established"] = True
            with self.assertRaises(ValueError):
                effects.validate(example, altered, execution, program, "subject.py")

    def test_model_agreement_cannot_replace_the_independently_expected_python_exception(self):
        example, program, report, _ = self.fixture()
        fabricated = [{"sent": [17], "raised": None}, {"sent": [31], "raised": None}]
        with self.assertRaisesRegex(ValueError, "independent Python execution"):
            effects.validate(example, report, fabricated, program, "subject.py")


if __name__ == "__main__":
    unittest.main()
