#!/usr/bin/env python3
"""Reject invented, malformed or inadmissible failing inputs before replay."""
import unittest

from python_symbolic_cases import admitted, counterexample, inputs, satisfies, symbolic_observation


class CounterexampleTests(unittest.TestCase):
    def test_positional_negative_integer_and_result_suffix(self):
        self.assertEqual(counterexample("error: false when calling unchecked_bounds(0, 5, -5) (which returns 5)",
                                        "unchecked_bounds"), {"x": 0, "lower": 5, "upper": -5})

    def test_keyword_arguments(self):
        self.assertEqual(counterexample("false when calling upper_only(upper=-1, x=0)", "upper_only"),
                         {"upper": -1, "x": 0})

    def test_executable_or_non_integer_tool_text_is_rejected(self):
        for expression in ("x=run(), lower=0, upper=1", "0, False, 1", "0, '1', 2",
                           "*[0, 1, 2]", "x=0, **{'lower': 1, 'upper': 2}"):
            with self.subTest(expression=expression), self.assertRaises((ValueError, SyntaxError)):
                counterexample("false when calling unchecked_bounds(" + expression + ")", "unchecked_bounds")

    def test_incomplete_or_ambiguous_calls_are_rejected(self):
        for output in ("", "false when calling unchecked_bounds(0, 1)",
                       "false when calling unchecked_bounds(0, x=1, lower=2, upper=3)",
                       "false when calling unchecked_bounds(0, 1, 2, 3)",
                       "when calling unchecked_bounds(0, 1, 2)\nwhen calling unchecked_bounds(0, 1, 2)"):
            with self.subTest(output=output), self.assertRaises(ValueError):
                counterexample(output, "unchecked_bounds")

    def test_an_input_outside_the_requirement_cannot_refute_it(self):
        with self.assertRaisesRegex(ValueError, "input requirement"):
            counterexample("false when calling ordered(0, 5, -5)", "ordered")

    def test_generated_inputs_preserve_reversed_bounds(self):
        candidates = inputs("unchecked_bounds", "generated")
        self.assertEqual(len(candidates), 343)
        self.assertTrue(any(row["lower"] > row["upper"] for row in candidates))
        self.assertTrue(all(admitted("unchecked_bounds", row) for row in candidates))
        self.assertTrue(any(not admitted("ordered", row) for row in candidates))

    def test_upper_only_is_not_a_lower_bound(self):
        arguments = {"x": -10, "upper": 5}
        self.assertTrue(satisfies("upper_only", arguments, -10))
        self.assertFalse(satisfies("upper_only", arguments, 5))

    def test_a_stop_cannot_become_a_success_even_with_success_text(self):
        for reason in ("wall_seconds", "cpu_seconds", "rss_bytes", "monitor_error"):
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, "stopped"):
                symbolic_observation({"exit_code": 0, "stop_reason": reason,
                                      "stdout": "checks.py:6: info: Confirmed over all paths."}, "ordered")

    def test_missing_checks_and_impossible_preconditions_remain_unfinished(self):
        for output in ("", "checks.py:6: info: Unable to meet precondition.",
                       "checks.py:6: info: Confirmed over all paths.\nchecks.py:7: error: other failure"):
            with self.subTest(output=output), self.assertRaises(ValueError):
                symbolic_observation({"exit_code": 0, "stop_reason": None, "stdout": output}, "ordered")

    def test_no_counterexample_is_only_a_limited_observation(self):
        self.assertIsNone(symbolic_observation({"exit_code": 0, "stop_reason": None,
                                               "stdout": "checks.py:6: info: Not confirmed."}, "ordered"))


if __name__ == "__main__":
    unittest.main()
