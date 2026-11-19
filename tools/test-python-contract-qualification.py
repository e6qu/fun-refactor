"""Reject stale acceptance counts and missing renamed Python examples."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('qualification', Path(__file__).with_name('python-contract-qualification.py'))
qualification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualification)


class ContractQualification(unittest.TestCase):
    def setUp(self):
        self.summary = {'contract_complete': True, 'cases': 4, 'incomplete_cases': 2}
        self.catalog = {'obligations': [{'id': 'B.contract', 'kind': 'semantic-contract', 'requires': [{
            'evidence': 'python-contract', 'assertions': [{'pointer': '/summary/' + name, 'equals': value}
                                                       for name, value in self.summary.items()]}]}]}
        self.assertions = self.catalog['obligations'][0]['requires'][0]['assertions']
        self.cases = [{'id': name + '/' + variant, 'variant': variant, 'rules_checked': ['calls'],
                       'expected_model': {'outcome': 'report', 'complete': complete}}
                      for name, complete in [('supported', True), ('unknown', False)]
                      for variant in ['original', 'moved']]

    def test_matching_counts_and_paired_examples_pass(self):
        qualification.check_catalog(self.catalog, self.summary)
        self.assertEqual(qualification.rule_coverage(self.cases, {'calls': 'description'}),
                         {'calls': {'examples': 2, 'cases': 4, 'complete': 2, 'incomplete': 2, 'refused': 0}})

    def test_newly_complete_cases_need_an_explicit_catalog_review(self):
        before = copy.deepcopy(self.catalog)
        with self.assertRaisesRegex(ValueError, 'catalog=2, audited=0'):
            qualification.check_catalog(self.catalog, {**self.summary, 'incomplete_cases': 0})
        self.assertEqual(self.catalog, before)

    def test_missing_duplicate_and_false_assertions_refuse(self):
        original = copy.deepcopy(self.assertions)
        for changed in [original[:-1], original + [original[-1]],
                        [{**item, 'equals': 1} if item['pointer'].endswith('contract_complete') else item for item in original]]:
            self.assertions[:] = changed
            with self.subTest(assertions=changed), self.assertRaises(ValueError):
                qualification.check_catalog(self.catalog, self.summary)

    def test_matching_failure_does_not_qualify(self):
        self.assertions[0]['equals'] = False
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            qualification.check_catalog(self.catalog, {**self.summary, 'contract_complete': False})

    def test_wrong_evidence_and_duplicate_gates_refuse(self):
        self.catalog['obligations'][0]['requires'][0]['evidence'] = 'other'
        with self.assertRaisesRegex(ValueError, 'selection'):
            qualification.check_catalog(self.catalog, self.summary)
        self.catalog['obligations'] *= 2
        with self.assertRaisesRegex(ValueError, 'one Python'):
            qualification.check_catalog(self.catalog, self.summary)

    def test_missing_duplicate_and_changed_moved_cases_refuse(self):
        for cases in [self.cases[:-1], self.cases + [self.cases[0]],
                      [{**case, 'expected_model': {'outcome': 'report', 'complete': False}}
                       if case['id'] == 'supported/moved' else case for case in self.cases]]:
            with self.subTest(cases=cases), self.assertRaises(ValueError):
                qualification.rule_coverage(cases, {'calls': 'description'})

    def test_unknown_rules_and_unexercised_rules_refuse(self):
        for rules in [{'different': 'description'}, {'calls': 'description', 'extra': 'description'}]:
            with self.subTest(rules=rules), self.assertRaises(ValueError):
                qualification.rule_coverage(self.cases, rules)

    def test_refusals_are_counted_separately(self):
        for case in self.cases[2:]:
            case['expected_model'] = {'outcome': 'refused', 'exit_code': 1}
        counts = qualification.rule_coverage(self.cases, {'calls': 'description'})['calls']
        self.assertEqual((counts['complete'], counts['incomplete'], counts['refused']), (2, 0, 2))


if __name__ == '__main__':
    unittest.main()
