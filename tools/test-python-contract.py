#!/usr/bin/env python3
"""Reject false contract claims and corrupted retained Python observations."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('contract', ROOT / 'tools/python-contract.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
REPORT = ROOT / 'tests/agent-eval/results/2026-10-11-coverage-python-contract/result.json'


class PythonContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        override = os.environ.get('FR_PYTHON_CONTRACT_REPORT')
        if override:
            if os.environ.get('GITHUB_ACTIONS') != 'true':
                raise ValueError('unretained reports may only be tested on GitHub')
            cls.report = json.loads(Path(override).read_text())
        else:
            manifest = json.loads((REPORT.parent / 'manifest.json').read_text())
            if manifest['files'] != {'result.json': hashlib.sha256(REPORT.read_bytes()).hexdigest()}:
                raise ValueError('contract report digest changed')
            cls.report = json.loads(REPORT.read_text())
            if (manifest['collection']['source_commit'] != cls.report['source_commit']
                    or manifest['binary_sha256'] != cls.report['binary_sha256']):
                raise ValueError('contract collection identity changed')

    def changed(self, name='unicode-local/moved'):
        value = copy.deepcopy(self.report)
        row = next(row for row in value['cases'] if row['case']['id'] == name)
        return value, row

    def reject(self, value):
        with self.assertRaises((ValueError, AssertionError)):
            contract.audit(value)

    def test_complete_inventory_and_independent_outcomes(self):
        self.assertEqual(contract.audit(self.report), contract.summary())
        self.assertEqual(set(contract.RULES), {rule for case in contract.EXAMPLES for rule in case['rules_checked']})

    def test_missing_or_duplicate_case(self):
        for duplicate in (False, True):
            value, _ = self.changed()
            value['cases'].pop()
            if duplicate:
                value['cases'].append(copy.deepcopy(value['cases'][0]))
            self.reject(value)

    def test_declared_scope_cannot_narrow(self):
        value, _ = self.changed()
        del value['rules']['boundaries']
        self.reject(value)

    def test_limits_cannot_disappear(self):
        value, _ = self.changed()
        value['limits'] = 'Full Python correctness'
        self.reject(value)

    def test_source_and_grammar_bindings(self):
        for path in ('src/project/dataflow.rs', 'grammars/python/src/parser.c', 'tools/python_contract_cases.py'):
            value, _ = self.changed()
            value['source_bindings'][path] = 'stale'
            self.reject(value)

    def test_runtime_events_and_order(self):
        for field, replacement in [('events', []), ('returned', 0), ('raised', 'TypeError')]:
            value, row = self.changed()
            row['runtime'][field] = replacement
            self.reject(value)
        value, row = self.changed('keyword-order/moved')
        row['runtime']['events'].reverse()
        self.reject(value)

    def test_runtime_expectation_cannot_be_rewritten(self):
        value, row = self.changed()
        row['case']['expected_runtime']['returned'] = 0
        row['runtime']['returned'] = 0
        self.reject(value)

    def test_boolean_runtime_cannot_become_integer(self):
        value, row = self.changed('and-skip/moved')
        row['runtime']['returned'] = 0
        self.reject(value)

    def test_report_rule_or_input_digest_cannot_change(self):
        for field in ('rules_digest', 'input_digest'):
            value, row = self.changed()
            row['flow'][field] = 'stale'
            self.reject(value)

    def test_documented_model_difference_cannot_disappear(self):
        value, row = self.changed('implicit-exception/moved')
        row['case']['difference'] = None
        self.reject(value)

    def test_configuration_cannot_choose_an_answer(self):
        value, row = self.changed()
        row['configuration']['sources'] = []
        self.reject(value)

    def test_false_positive_and_missing_witness(self):
        for name in ('overwrite/moved', 'unicode-local/moved'):
            value, row = self.changed(name)
            row['flow']['witnesses'] = [{'invented': True}] if not row['flow']['witnesses'] else []
            self.reject(value)

    def test_unsupported_cannot_be_complete(self):
        for name in ('bare-raise', 'handler', 'budget', 'container', 'unbound-local', 'shadow-source'):
            value, row = self.changed(name + '/moved')
            row['flow'].update(complete=True, cutoffs=[])
            self.reject(value)

    def test_cutoff_reason_cannot_change(self):
        value, row = self.changed('bare-raise/moved')
        row['flow']['cutoffs'] = ['unrelated']
        self.reject(value)

    def test_proven_path_claim_refused(self):
        for nested in (False, True):
            value, row = self.changed()
            target = row['flow']['witnesses'][0] if nested else row['flow']
            target['claim'] = 'proven-executable-path'
            self.reject(value)

    def test_assumptions_cannot_disappear(self):
        value, row = self.changed()
        row['flow']['assumptions'] = []
        self.reject(value)

    def test_explicit_raise_cannot_invent_a_normal_return(self):
        value, row = self.changed('raise-before-next-argument/moved')
        row['flow']['completion']['normal_return'] = True
        self.reject(value)

    def test_implicit_exceptions_cannot_be_claimed_as_modeled(self):
        value, row = self.changed('implicit-exception/moved')
        row['flow']['completion']['may_raise'] = True
        self.reject(value)

    def test_wrong_source_revision(self):
        value, row = self.changed()
        row['flow']['revision'] = 'stale'
        self.reject(value)

    def test_trace_locations_and_revisions(self):
        for field in ('byte', 'column', 'file', 'revision'):
            value, row = self.changed()
            occurrence = row['flow']['witnesses'][0]['trace']['occurrences'][0]
            if field == 'byte':
                occurrence['location']['span']['start'] += 1
            elif field == 'column':
                occurrence['location']['range']['start']['col'] += 1
            else:
                occurrence['path' if field == 'file' else field] = 'wrong'
            self.reject(value)

    def test_wrong_entry_selection(self):
        value, row = self.changed()
        row['find']['rows'][0][row['find']['columns'].index('path')] = 'wrong.py'
        self.reject(value)

    def test_duplicate_witnesses(self):
        value, row = self.changed()
        row['flow']['witnesses'].append(copy.deepcopy(row['flow']['witnesses'][0]))
        self.reject(value)

    def test_valid_but_wrong_source_call(self):
        value, row = self.changed('comparison-order/moved')
        calls = contract.ast_locations(row['case'])[1]
        first = min(span for span, name in calls.items() if name == row['case']['names']['source'])
        occurrence = row['flow']['witnesses'][0]['trace']['occurrences'][0]
        occurrence['location'] = contract.profile.location(row['case']['source'], *first)
        self.reject(value)

    def test_wrong_command_or_budget(self):
        for field in ('command', 'budget', 'source', 'context'):
            value, row = self.changed()
            if field == 'command':
                row['commands'][1][3] = '--imports'
            elif field == 'budget':
                row['flow']['inputs']['budget']['steps'] = 1
            elif field == 'source':
                row['flow']['inputs']['source']['digest'] = 'stale'
            else:
                row['flow']['inputs']['context'] = 'different'
            self.reject(value)

    def test_typed_contract_rejects_invented_semantics(self):
        for name in ('expression_control', 'assignment_control', 'call_binding'):
            value, row = self.changed()
            row['flow'][name]['schema'] = 'invented'
            self.reject(value)

    def test_collection_identity_required(self):
        for key in ('source_commit', 'binary_sha256'):
            value, _ = self.changed()
            value[key] = 'unknown'
            self.reject(value)

    def test_module_source_inventory_and_candidate_bytes(self):
        for mutation in ('missing', 'extra', 'bytes', 'candidate'):
            value, row = self.changed('import-absolute-selected/moved')
            modules = row['flow']['inputs']['modules']
            if mutation == 'missing':
                modules['files'].pop('engine.py')
            elif mutation == 'extra':
                modules['files']['invented.py'] = '0' * 64
            elif mutation == 'bytes':
                modules['files']['engine.py'] = '0' * 64
            else:
                modules['lookups'][0]['candidates']['engine.pyi'] = {'status': 'directory'}
            row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
            self.reject(value)

    def test_callee_path_and_signature_cannot_change(self):
        for mutation in ('path', 'name', 'kind', 'site', 'default'):
            value, row = self.changed('import-keyword-default/moved')
            functions = row['flow']['function_summaries']['functions']
            helper = functions['engine.py::transfer']
            if mutation == 'path':
                functions['renamed.py::transfer'] = functions.pop('engine.py::transfer')
            elif mutation in ('name', 'kind'):
                helper['signature'][0][mutation] = 'wrong'
            else:
                helper['signature'][0][mutation]['path'] = 'renamed.py'
            self.reject(value)

    def test_import_witness_cannot_borrow_another_file_span(self):
        value, row = self.changed('import-absolute-selected/moved')
        points = row['flow']['witnesses'][0]['trace']['occurrences']
        next(point for point in points if point['path'] == 'engine.py')['path'] = 'renamed.py'
        self.reject(value)

    def test_import_oracle_isolated_between_conflicting_modules(self):
        selected = next(case for case in contract.cases() if case['id'] == 'import-absolute-selected/original')
        discarded = next(case for case in contract.cases() if case['id'] == 'import-absolute-discarded/original')
        contract.oracle_json.cache_clear()
        for case in (selected, discarded, selected):
            self.assertEqual(contract.oracle(case), case['expected_runtime'])

    def test_oracle_cache_does_not_expose_mutable_answers(self):
        case = contract.cases()[0]
        first = contract.oracle(case)
        first['events'].clear()
        self.assertEqual(contract.oracle(case), case['expected_runtime'])

    def test_definition_failure_is_not_an_entry_failure(self):
        value, row = self.changed('default-definition-failure/moved')
        row['runtime']['phase'] = 'entry'
        self.reject(value)

    def test_recursive_completion_cannot_be_invented(self):
        value, row = self.changed('mutual-no-return/moved')
        row['flow']['completion']['normal_return'] = True
        self.reject(value)

    def test_summary_cannot_overstate_coverage(self):
        value, _ = self.changed()
        value['summary']['cases'] += 2
        self.reject(value)


if __name__ == '__main__':
    unittest.main()
