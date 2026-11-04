"""Keep diagnostic failures distinct from accepted proofs and broken controls."""
import unittest

from agent_eval import page_count_diagnosis as diagnosis


class PageCountDiagnosis(unittest.TestCase):
    def test_each_control_changes_only_its_named_step_in_the_original_reference(self):
        reference = diagnosis.REFERENCE.read_text()
        drafts = diagnosis.drafts(reference)
        self.assertEqual(drafts['reference'], drafts['reference-again'])
        for name, (before, after) in diagnosis.MUTATIONS.items():
            self.assertEqual(drafts[name], reference.replace(before, after, 1))
            with self.assertRaisesRegex(ValueError, 'exactly one proof step'):
                diagnosis.drafts(reference.replace(before, after, 1))

    def test_missing_boolean_results_and_inconsistent_receipts_cannot_pass(self):
        for name in ('reference', 'reference-again', 'integer-instead-of-proof'):
            accepted = name != 'integer-instead-of-proof'
            valid = {'passed': accepted, 'receipt': {} if accepted else None}
            diagnosis.validate(name, valid)
            for invalid in ({}, {'passed': int(accepted)}, {**valid, 'passed': not accepted},
                            {**valid, 'receipt': None if accepted else {}}):
                with self.assertRaises(ValueError):
                    diagnosis.validate(name, invalid)


if __name__ == '__main__':
    unittest.main()
