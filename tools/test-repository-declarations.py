"""Reject missing declaration cells and altered independent scalar specifications."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('declarations', ROOT / 'tools/python-fixed-arity-probes.py')
declarations = importlib.util.module_from_spec(spec)
spec.loader.exec_module(declarations)


class DeclarationEvidence(unittest.TestCase):
    def answer(self, case, complete, witnesses, cutoffs, context):
        return {'case': case, 'context': context, 'invocations': [{}, {
            'exit_code': 0, 'report': {
                'schema': 'fr-dataflow-1', 'semantics': 'python-scalar-summaries',
                'claim': 'possible-value-propagation', 'complete': complete,
                'witnesses': witnesses, 'cutoffs': cutoffs}}]}

    def test_a_correct_original_package_answer_can_improve(self):
        helper = declarations.HELPERS[1]
        for case, witnesses, no_path in [('positive', ['input-to-output'], False),
                                         ('overwritten', [], True)]:
            row = self.answer(case, True, witnesses, [], 'full-package')
            self.assertEqual(declarations.declaration_outcome(row, helper),
                             {'status': 'complete', 'no_path': no_path})

    def test_complete_package_answers_must_match_the_independent_operation(self):
        for case, witnesses in [('positive', []), ('overwritten', ['unexpected-connection'])]:
            row = self.answer(case, True, witnesses, [], 'full-package')
            with self.subTest(case=case), self.assertRaisesRegex(ValueError, 'independent specification'):
                declarations.declaration_outcome(row, declarations.HELPERS[1])
        row = self.answer('positive', True, ['unexpected-connection'], [], 'full-package')
        with self.assertRaisesRegex(ValueError, 'independent specification'):
            declarations.declaration_outcome(row, declarations.HELPERS[0])

    def test_unknown_work_and_budget_stops_cannot_be_promoted(self):
        for context in declarations.CONTEXTS:
            for case in ('unknown', 'steps'):
                row = self.answer(case, True, [], [], context)
                with self.subTest(context=context, case=case), self.assertRaisesRegex(ValueError, 'unsupported effects'):
                    declarations.declaration_outcome(row, declarations.HELPERS[1])

    def test_an_incomplete_empty_answer_does_not_establish_no_connection(self):
        row = self.answer('overwritten', False, [], ['module-effects-unchecked'], 'full-package')
        self.assertEqual(declarations.declaration_outcome(row, declarations.HELPERS[1]),
                         {'status': 'incomplete', 'no_path': False})
        for complete, cutoffs in [(True, ['module-effects-unchecked']), (False, [])]:
            row = self.answer('overwritten', complete, [], cutoffs, 'full-package')
            with self.subTest(complete=complete), self.assertRaises(ValueError):
                declarations.declaration_outcome(row, declarations.HELPERS[1])

    def inventory(self):
        return {'declaration_inspection': {helper['id']: {} for helper in declarations.HELPERS},
                'declaration_cases': [
                    {'helper': helper['id'], 'project': helper['project'],
                     'context': context, 'variant': variant, 'case': case}
                    for helper in declarations.HELPERS for context in declarations.contexts(helper)
                    for variant in declarations.VARIANTS for case in declarations.flow.BODIES]}

    def test_missing_or_duplicate_cells_cannot_narrow_coverage(self):
        for duplicate in (False, True):
            value = self.inventory()
            value['declaration_cases'].pop()
            if duplicate:
                value['declaration_cases'].append(copy.deepcopy(value['declaration_cases'][0]))
            with self.assertRaisesRegex(ValueError, 'missing or duplicate'):
                declarations.declaration_inventory(value)

    def test_a_helper_cannot_claim_another_repository(self):
        value = self.inventory()
        value['declaration_cases'][0]['project'] = 'unfamiliar'
        with self.assertRaisesRegex(ValueError, 'repository identity'):
            declarations.declaration_inventory(value)

    def sources(self, root, addition, companion):
        helper = declarations.HELPERS[-1]
        path = root / helper['path']
        path.parent.mkdir(parents=True)
        path.write_text(addition)
        (root / helper['companion_path']).write_text(companion)
        return helper

    def test_changed_scalar_operation_cannot_rewrite_the_specification(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = self.sources(root, 'def add(a, b):\n    return a - b\n',
                                  'def range_(*args):\n    return args\n')
            with self.assertRaisesRegex(ValueError, 'independent specification'):
                declarations.inspection(root, helper)

    def test_multiple_runtime_definitions_refuse_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = self.sources(root, 'def add(a, b):\n    return a + b\ndef add(a, b):\n    return 0\n',
                                  'def range_(*args):\n    return args\n')
            with self.assertRaisesRegex(ValueError, 'selection is ambiguous'):
                declarations.inspection(root, helper)

    def test_companion_default_effects_cannot_enter_an_inert_capsule(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = self.sources(root, 'def add(a, b):\n    return a + b\n',
                                  'def range_(marker=effect(), *args):\n    return args\n')
            with self.assertRaisesRegex(ValueError, 'inert signature'):
                declarations.inspection(root, helper)


if __name__ == '__main__':
    unittest.main()
