#!/usr/bin/env python3
"""Reject missing cases, false claims and altered source/compiler observations."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('profile', ROOT / 'tools/compiler-profile.py')
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)
REPORT = ROOT / 'tests/agent-eval/results/2026-10-10-compiler-profile/result.json'


class CompilerProfile(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        override = os.environ.get('FR_COMPILER_PROFILE_REPORT')
        if override:
            if os.environ.get('GITHUB_ACTIONS') != 'true':
                raise ValueError('unretained reports may only be tested on GitHub')
            cls.report = json.loads(Path(override).read_text())
        else:
            manifest = json.loads((REPORT.parent / 'manifest.json').read_text())
            if manifest['files'] != {'result.json': hashlib.sha256(REPORT.read_bytes()).hexdigest()}:
                raise ValueError('retained compiler profile digest changed')
            cls.report = json.loads(REPORT.read_text())
            if (cls.report['source_commit'] != manifest['collection']['source_commit']
                    or cls.report['binary_sha256'] != manifest['binary_sha256']):
                raise ValueError('compiler profile collection identity changed')

    def changed(self, feature='python-repeated', variant='renamed-relocated'):
        value = copy.deepcopy(self.report)
        row = next(r for r in value['cases'] if r['case']['id'] == feature + '/' + variant)
        return value, row

    def reject(self, value):
        with self.assertRaises((ValueError, AssertionError)):
            profile.audit(value)

    def change_report(self, row, key, change):
        report = json.loads(row['events'][key]['stdout'])
        change(report)
        row['events'][key]['stdout'] = json.dumps(report)

    def test_declared_profile(self):
        result = profile.audit(self.report)
        self.assertEqual(result['cases'], 18)
        self.assertEqual(result['features'], 11)
        self.assertEqual(result['renamed_relocated_cases'], 9)

    def test_missing_and_duplicate_cases(self):
        for duplicate in (False, True):
            value, _ = self.changed()
            value['cases'].pop()
            if duplicate:
                value['cases'].append(copy.deepcopy(value['cases'][0]))
            self.reject(value)

    def test_feature_table_cannot_narrow(self):
        value, _ = self.changed()
        del value['profile']['rust-shadow']
        self.reject(value)

    def test_source_bindings_cannot_drift(self):
        value, _ = self.changed()
        value['source_bindings']['src/span.rs'] = 'stale'
        self.reject(value)

    def test_parser_build_inputs_cannot_drift(self):
        for path in ('grammars/python/src/parser.c', 'grammars/python/build.rs', 'Cargo.toml'):
            value, _ = self.changed()
            value['source_bindings'][path] = 'stale'
            self.reject(value)

    def test_source_and_configuration_cannot_change(self):
        value, row = self.changed('python-positive')
        row['case']['rules']['sources'] = []
        self.reject(value)

    def test_repeated_calls_cannot_share_coordinates(self):
        value, row = self.changed()
        def change(report):
            calls = [r for r in report['items'] if (r.get('callee') or {}).get('name') == row['case']['callee']]
            calls[1]['site'] = copy.deepcopy(calls[0]['site'])
        self.change_report(row, 'calls', change)
        self.reject(value)

    def test_unicode_columns_cannot_be_byte_columns(self):
        value, row = self.changed()
        def change(report):
            calls = [r for r in report['items'] if (r.get('callee') or {}).get('name') == row['case']['callee']]
            calls[-1]['site']['origins']['occurrence']['location']['range']['start']['col'] += 1
        self.change_report(row, 'calls', change)
        self.reject(value)

    def test_runtime_dispatch_cannot_be_claimed(self):
        value, row = self.changed()
        self.change_report(row, 'calls', lambda r: r['provenance'].update(claim='runtime-dispatch'))
        self.reject(value)

    def test_rust_shadow_cannot_resolve_to_global(self):
        value, row = self.changed('rust-shadow')
        self.change_report(row, 'calls', lambda r: r['items'][0].update(callee={'name': row['case']['callee']}, status='resolved'))
        self.reject(value)

    def test_rust_flow_cannot_pass(self):
        value, row = self.changed('rust-repeated')
        row['events']['flow']['exit_code'] = 0
        row['events']['flow']['stdout'] = json.dumps({
            'revision': json.loads(row['events']['find']['stdout'])['revision'],
            'complete': True, 'witnesses': []})
        self.reject(value)

    def test_absent_origins_cannot_become_exact(self):
        for feature in ('python-normalized',):
            value, row = self.changed(feature)
            self.change_report(row, 'origins', lambda r: r['origins']['items'][0]['origins'].update(status='exact'))
            self.reject(value)

    def test_shadowed_body_cannot_pass_with_absent_origins(self):
        value, row = self.changed('python-shadow')
        row['events']['origins']['exit_code'] = 0
        found = json.loads(row['events']['find']['stdout'])
        row['events']['origins']['stdout'] = json.dumps({
            'revision': found['revision'], 'model': {'items': [{'kind': 'function', 'value': {
                'name': row['case']['target'], 'body': [{'kind': 'return', 'value': {'kind': 'int', 'value': '1'}}]}}]},
            'origins': {'complete': True, 'mutation_authority': False,
                        'items': [{'origins': {'status': 'absent'}}]}})
        self.reject(value)

    def test_shadow_refusal_requires_a_selection_reason(self):
        value, row = self.changed('python-shadow')
        row['events']['origins']['stderr'] = 'unrelated command failure'
        self.reject(value)

    def test_incomplete_flow_cannot_become_complete(self):
        for feature in ('python-local', 'python-handler'):
            value, row = self.changed(feature)
            self.change_report(row, 'flow', lambda r: r.update(complete=True, cutoffs=[]))
            self.reject(value)

    def test_overwrite_cannot_invent_a_witness(self):
        value, row = self.changed('python-overwrite')
        self.change_report(row, 'flow', lambda r: r.update(witnesses=[{'invented': True}]))
        self.reject(value)

    def test_flow_trace_cannot_invent_coordinates(self):
        value, row = self.changed('python-positive')
        def change(report):
            report['witnesses'][0]['trace']['occurrences'][0]['location']['span']['start'] += 1
        self.change_report(row, 'flow', change)
        self.reject(value)

    def test_possible_flow_cannot_be_promoted_to_a_proof(self):
        value, row = self.changed('python-positive')
        self.change_report(row, 'flow', lambda r: r['witnesses'][0].update(claim='proven-path'))
        self.reject(value)

    def test_queries_cannot_mix_source_revisions(self):
        value, row = self.changed()
        self.change_report(row, 'origins', lambda r: r.update(revision='stale'))
        self.reject(value)

    def test_python_runtime_cannot_change(self):
        value, row = self.changed()
        row['oracle']['runtime'] = [0, 0, 0]
        self.reject(value)

    def test_rust_runtime_cannot_change(self):
        value, row = self.changed('rust-shadow')
        row['oracle']['execute']['stdout'] = '3\n'
        self.reject(value)

    def test_command_target_cannot_change(self):
        value, row = self.changed()
        row['events']['origins']['argv'][6] = 'unrelated'
        self.reject(value)

    def test_compiler_rejection_cannot_become_acceptance(self):
        value, _ = self.changed()
        value['compiler']['cases']['strict']['checks']['passed'] = True
        nested = value['compiler']
        nested['evidence_digest'] = profile.compiler.digest([nested['cases'], nested['clipped']['report'], nested['cargo']['report'], nested['plan']])
        self.reject(value)

    def test_nested_binary_identity_cannot_change(self):
        value, _ = self.changed()
        value['semantic']['binary_sha256'] = '0' * 64
        self.reject(value)

    def test_toolchain_identity_is_required(self):
        value, _ = self.changed()
        value['toolchains']['rustc']['sha256'] = ''
        self.reject(value)

    def test_summary_cannot_overclaim(self):
        value, _ = self.changed()
        value['summary']['cases'] += 1
        self.reject(value)


if __name__ == '__main__':
    unittest.main()
