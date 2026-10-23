#!/usr/bin/env python3
"""Check the independent arithmetic and reject altered retained answers."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("checked_translation", Path(__file__).with_name("checked-translation.py"))
translation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(translation)


class Arithmetic(unittest.TestCase):
    def test_invalid_inputs_never_divide_and_large_results_do_not_round_through_float(self):
        pairs = [(11, 5), (10, 5), (0, 5), (-1, 0), (1, 0), (1, -1),
                 (2**63 - 1, 2), (2**63 - 1, 3), (2**63 - 1, 2**63 - 1)]
        self.assertEqual(translation.expected(pairs), b"3\n2\n0\n-1\n-1\n-1\n4611686018427387904\n3074457345618258603\n1\n")

    def test_contract_covers_the_full_declared_cartesian_product_and_extremes(self):
        contract = json.loads((translation.FIXTURE / "contract.json").read_text())
        pairs = translation.cases(contract)
        self.assertEqual(len(pairs), 9041)
        self.assertEqual(len(set(pairs)), len(pairs))
        for pair in [(-2, -2), (255, 32), (2**63 - 1, 1), (-(2**63), 1)]:
            self.assertIn(pair, pairs)

    def test_a_fresh_file_hash_does_not_make_wrong_answers_acceptable(self):
        contract = json.loads((translation.FIXTURE / "contract.json").read_text())
        pairs = translation.cases(contract)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            answer = directory / "answers.txt"
            answer.write_bytes(translation.expected(pairs))
            commands = directory / "commands.json"
            commands.write_text('[{"exit_code": 0}]')
            rows = [{"direction": direction, "renamed_and_moved": renamed, "passed": True,
                     "checked_inputs": len(pairs), "source_answers": answer.name,
                     "translated_answers": answer.name, "commands": commands.name,
                     "files": {answer.name: translation.digest(answer), commands.name: translation.digest(commands)}}
                    for direction in contract["directions"] for renamed in (False, True)]
            result = {"schema": "fr-checked-translation", "passed": True, "source_bindings": {},
                      "coverage": translation.coverage(contract), "input_pairs": pairs, "live_model_calls": 0,
                      "runtime_benchmark": False, "translations": rows}
            report = directory / "result.json"
            report.write_text(json.dumps(result))
            with patch.object(translation, "bindings", return_value={}):
                translation.audit(directory)
                answer.write_bytes(answer.read_bytes().replace(b"4611686018427387904", b"4611686018427387903"))
                for row in rows:
                    row["files"][answer.name] = translation.digest(answer)
                report.write_text(json.dumps(result))
                with self.assertRaisesRegex(ValueError, "independent arithmetic"):
                    translation.audit(directory)


if __name__ == "__main__":
    unittest.main()
