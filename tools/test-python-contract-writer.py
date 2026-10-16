#!/usr/bin/env python3
"""Keep completed Python observations readable without rewriting earlier rows."""
from contextlib import ExitStack, redirect_stderr
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('contract', ROOT / 'tools/python-contract.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def initial():
    return {'schema': contract.SCHEMA, 'label': 'café → 結果',
            'cases': [], 'summary': {'contract_complete': False}}


class ReportWriter(unittest.TestCase):
    def test_each_checkpoint_is_json_with_exact_completed_rows(self):
        value = initial()
        rows = [{'id': 'é', 'flow': {'text': '雪\n\"quoted\"'}},
                {'id': 'refused', 'flow': None, 'error': {'message': '範囲外'}},
                {'id': 'short', 'flow': {}}]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'nested/result.json'
            with contract.collecting_report(value, path) as append:
                self.assertEqual(json.loads(path.read_bytes()), value)
                for row in rows:
                    value['cases'].append(row)
                    append(row)
                    self.assertEqual(json.loads(path.read_bytes()), value)
            self.assertEqual(json.loads(path.read_bytes()), value)

    def test_python_interruption_keeps_completed_rows_and_incomplete_status(self):
        value = initial()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'result.json'
            with self.assertRaises(KeyboardInterrupt):
                with contract.collecting_report(value, path) as append:
                    append({'id': 'done'})
                    raise KeyboardInterrupt
            saved = json.loads(path.read_bytes())
            self.assertEqual(saved['cases'], [{'id': 'done'}])
            self.assertIs(saved['summary']['contract_complete'], False)

    def test_process_exit_between_rows_keeps_the_flushed_checkpoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'result.json'
            program = '''
import importlib.util, os, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location('contract', sys.argv[1])
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
value = {'cases': [], 'summary': {'contract_complete': False}}
with contract.collecting_report(value, Path(sys.argv[2])) as append:
    append({'id': 'completed', 'text': '雪'})
    os._exit(17)
'''
            result = subprocess.run([sys.executable, '-B', '-c', program,
                                     str(ROOT / 'tools/python-contract.py'), str(path)],
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 17, result.stderr)
            self.assertEqual(json.loads(path.read_bytes()), {
                'cases': [{'id': 'completed', 'text': '雪'}], 'summary': {'contract_complete': False}})

    def test_unserializable_row_does_not_damage_the_previous_checkpoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'result.json'
            with contract.collecting_report(initial(), path) as append:
                append({'id': 'completed'})
                before = path.read_bytes()
                with self.assertRaises(TypeError):
                    append({'invalid': object()})
                self.assertEqual(path.read_bytes(), before)
                append({'id': 'next'})
            self.assertEqual(json.loads(path.read_bytes())['cases'],
                             [{'id': 'completed'}, {'id': 'next'}])

    def test_serialization_visits_each_row_once_as_the_report_grows(self):
        original = contract.json.dumps
        serialized = []

        def encode(value, **kwargs):
            serialized.append(value)
            return original(value, **kwargs)

        rows = [{'id': index, 'payload': 'é' * 1024} for index in range(32)]
        with tempfile.TemporaryDirectory() as temporary, patch.object(contract.json, 'dumps', side_effect=encode):
            with contract.collecting_report(initial(), Path(temporary) / 'result.json') as append:
                for row in rows:
                    append(row)
        self.assertEqual(serialized[1:], rows)
        self.assertNotIn('cases', serialized[0])

    def test_no_output_does_not_serialize_or_create_files(self):
        with patch.object(contract.json, 'dumps') as encode:
            with contract.collecting_report(initial(), None) as append:
                append({'unused': object()})
        encode.assert_not_called()

    def test_completed_or_nonempty_reports_cannot_start_a_collection(self):
        for value in ({'cases': [{}], 'summary': {'contract_complete': False}},
                      {'cases': [], 'summary': {'contract_complete': True}}):
            with self.assertRaisesRegex(ValueError, 'empty, incomplete'):
                with contract.collecting_report(value, None):
                    self.fail('invalid initial report accepted')


class CollectionCheckpoints(unittest.TestCase):
    def collect(self, path, *, audit_error=False, interrupt=False):
        cases = [{'id': name, 'files': {'main.py': '# example\n'}, 'names': {'entry': 'entry'}}
                 for name in ('first', 'second')]
        found = {'columns': ['handle'], 'rows': [['handle']]}
        responses = [SimpleNamespace(returncode=0, stderr='', stdout=json.dumps(found)),
                     SimpleNamespace(returncode=0, stderr='', stdout='{}'),
                     SimpleNamespace(returncode=0, stderr='', stdout=json.dumps(found)),
                     SimpleNamespace(returncode=1, stderr='refused', stdout='{"refused": true}')]
        if interrupt:
            responses[2] = subprocess.TimeoutExpired(['fr'], 30)
        seen = []

        def audit(row, case):
            checkpoint = json.loads(path.read_bytes())
            seen.append(checkpoint)
            self.assertEqual(checkpoint['cases'][-1], row)
            self.assertIs(checkpoint['summary']['contract_complete'], False)
            if audit_error and case['id'] == 'first':
                raise ValueError('wrong answer')

        with ExitStack() as stack:
            stack.enter_context(patch.dict(contract.os.environ, {'GITHUB_ACTIONS': 'true'}))
            for name, replacement in {'cases': lambda: cases, 'bindings': lambda: {},
                                      'file_digest': lambda path: '0' * 64,
                                      'configuration': lambda case: {},
                                      'flow_command': lambda *args: ['flow'],
                                      'oracle': lambda case: {'executed': case['id']},
                                      'audit_case': audit,
                                      'summary': lambda: {'contract_complete': True}}.items():
                stack.enter_context(patch.object(contract, name, replacement))
            stack.enter_context(patch.object(contract.subprocess, 'check_output', return_value='a' * 40))
            stack.enter_context(patch.object(contract.subprocess, 'run', side_effect=responses))
            stack.enter_context(redirect_stderr(io.StringIO()))
            value = contract.measure(Path(sys.executable), path)
        return value, seen

    def test_success_retains_refusals_and_only_finishes_after_the_audit(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'result.json'
            value, seen = self.collect(path)
            self.assertEqual([len(item['cases']) for item in seen], [1, 2])
            self.assertEqual(value['cases'][1]['error'], {'refused': True})
            self.assertIs(json.loads(path.read_bytes())['summary']['contract_complete'], False)
            contract.save(value, path)
            self.assertEqual(json.loads(path.read_bytes()), value)

    def test_failed_audit_retains_all_rows_and_the_failure_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'result.json'
            with self.assertRaisesRegex(ValueError, 'first: wrong answer'):
                self.collect(path, audit_error=True)
            value = json.loads(path.read_bytes())
            self.assertEqual(len(value['cases']), 2)
            self.assertEqual(value['summary'], {'contract_complete': False, 'errors': ['first: wrong answer']})

    def test_command_timeout_retains_only_finished_rows(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'result.json'
            with self.assertRaises(subprocess.TimeoutExpired):
                self.collect(path, interrupt=True)
            value = json.loads(path.read_bytes())
            self.assertEqual([row['case']['id'] for row in value['cases']], ['first'])
            self.assertIs(value['summary']['contract_complete'], False)


if __name__ == '__main__':
    unittest.main()
