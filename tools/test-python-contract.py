#!/usr/bin/env python3
"""Reject false contract claims and corrupted retained Python observations."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('contract', ROOT / 'tools/python-contract.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
REPORT = ROOT / 'tests/agent-eval/results/2026-10-16-incremental-reports-python-contract/result.json'


class RegistryAllocations(unittest.TestCase):
    def accepts(self, text):
        import ast
        from python_contract_allocations import audit_declaration
        statements = ast.parse(text).body
        return audit_declaration(statements[-1], statements)

    def test_callback_body_is_deferred_but_default_is_evaluated(self):
        self.assertTrue(self.accepts('table = dict(send=lambda: source())'))
        self.assertFalse(self.accepts('table = dict(send=lambda value=source(): value)'))

    def test_constructor_values_are_not_assumed_harmless(self):
        for expression in ('dict(send=source())', 'dict(missing)', 'dict(**{})', 'dict([(0, 1)])'):
            self.assertFalse(self.accepts('table = ' + expression))

    def test_dictionary_keys_cannot_run_custom_hashing(self):
        self.assertTrue(self.accepts("table = {('key', 0): lambda: missing()}"))
        self.assertFalse(self.accepts('table = {Key(): lambda: missing()}'))
        self.assertFalse(self.accepts('table = {[]: lambda: missing()}'))

    def test_replaced_builtin_is_not_accepted_by_spelling(self):
        for setup in ('dict = custom', 'def dict(**items):\n    return items',
                      'from helper import dict', 'from helper import *', '__builtins__ = custom'):
            self.assertFalse(self.accepts(setup + '\ntable = dict(send=lambda: 0)'))

    def test_literal_dictionary_does_not_call_a_shadowed_constructor(self):
        self.assertTrue(self.accepts("dict = custom\ntable = {'send': lambda: 0}"))

    def test_mutable_and_global_defaults_remain_outside_the_contract(self):
        for default in ('[]', '{}', 'missing', 'source()'):
            self.assertFalse(self.accepts('table = lambda value=' + default + ': value'))


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
        cls.authored_cases = staticmethod(contract.cases)
        encoded = json.dumps(contract.cases(), ensure_ascii=False)
        snapshots = patch.object(contract, 'cases', side_effect=lambda: json.loads(encoded))
        snapshots.start()
        cls.addClassCleanup(snapshots.stop)

    def test_authored_case_snapshots_match_source_and_do_not_share_mutable_data(self):
        first = contract.cases()
        expected = contract.profile.compiler.digest(self.authored_cases())
        self.assertEqual(contract.profile.compiler.digest(first), expected)
        first[0]['expected_runtime']['events'].clear()
        first[0]['files'].clear()
        self.assertEqual(contract.profile.compiler.digest(contract.cases()), expected)

    def changed(self, name='unicode-local/moved'):
        value = copy.deepcopy({key: item for key, item in self.report.items() if key != 'cases'})
        value['cases'] = list(self.report['cases'])
        index = next(index for index, row in enumerate(value['cases']) if row['case']['id'] == name)
        row = copy.deepcopy(value['cases'][index])
        value['cases'][index] = row
        return value, row

    def reject(self, value):
        with self.assertRaises((ValueError, AssertionError)):
            contract.audit(value)

    def test_corruption_fixtures_leave_the_retained_report_unchanged(self):
        before = contract.profile.compiler.digest(self.report)
        value, row = self.changed()
        value['source_bindings'].clear()
        value['cases'].pop()
        row['runtime']['events'].clear()
        row['flow']['witnesses'][0]['trace']['occurrences'][0]['location']['span']['start'] += 1
        self.assertEqual(contract.profile.compiler.digest(self.report), before)

    def test_complete_inventory_and_independent_outcomes(self):
        self.assertEqual(contract.audit(self.report), contract.summary())
        self.assertEqual(set(contract.RULES), {rule for case in contract.EXAMPLES for rule in case['rules_checked']})

    def test_repeated_audits_reuse_runtime_executions_and_still_check_report_rows(self):
        contract.oracle_json.cache_clear()
        self.assertEqual(contract.audit(self.report), contract.summary())
        first = contract.oracle_json.cache_info()
        self.assertEqual(contract.audit(self.report), contract.summary())
        second = contract.oracle_json.cache_info()
        self.assertEqual(second.misses, first.misses)
        self.assertGreater(second.hits, first.hits)
        self.assertEqual(first.currsize, len(contract.cases()))
        value, row = self.changed()
        row['runtime']['returned'] = 0
        self.reject(value)

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

    def test_imported_values_must_match_the_declared_kind_and_source_position(self):
        for case, kind in [('value-import-scalar', 'class'), ('value-import-class', 'literal'),
                           ('value-import-instance', 'literal'), ('type-registry-import', 'literal'),
                           ('builtin-base-import', 'literal')]:
            value, row = self.changed(case + '/original')
            row['flow']['inputs']['modules']['value_bindings'][0]['kind'] = kind
            row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
            self.reject(value)
        value, row = self.changed('value-import-reexport/original')
        binding = row['flow']['inputs']['modules']['value_bindings'][0]
        source = row['case']['files'][binding['path']]
        binding['offset'] = source.encode().index(b'def relay')
        row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
        self.reject(value)

    def test_runtime_events_and_order(self):
        for field, replacement in [('events', []), ('returned', 0), ('raised', 'TypeError')]:
            value, row = self.changed()
            row['runtime'][field] = replacement
            self.reject(value)
        value, row = self.changed('keyword-order/moved')
        row['runtime']['events'].reverse()
        self.reject(value)

    def test_postponed_annotations_keep_their_source_directive_and_negative_paths(self):
        for mutation in ('missing', 'candidate', 'importer'):
            value, row = self.changed('postponed-annotations-forward/moved')
            lookup = next(item for item in row['flow']['inputs']['modules']['lookups'] if item['future'])
            if mutation == 'missing':
                del lookup['future']
            elif mutation == 'candidate':
                lookup['candidates']['__future__.py']['status'] = 'symlink'
            else:
                lookup['importer'] = row['case']['path']
            row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
            self.reject(value)

    def test_annotation_text_cannot_replace_the_parameter_name_in_a_trace(self):
        value, row = self.changed('postponed-annotations-forward/moved')
        helper = next(path for path in row['case']['files'] if path != row['case']['path'])
        source = row['case']['files'][helper]
        definition = next(node for node in contract.ast.parse(source).body
                          if isinstance(node, contract.ast.FunctionDef))
        argument = definition.args.args[0]
        spans, _ = contract.ast_locations(row['case'], helper)
        start = sum(len(line) for line in source.encode().splitlines(keepends=True)[:argument.lineno - 1]) + argument.col_offset
        name_span = (start, start + len(argument.arg.encode()))
        assert name_span in spans
        for occurrence in row['flow']['witnesses'][0]['trace']['occurrences']:
            if occurrence['path'] == helper and occurrence['location']['span'] == {'start': name_span[0], 'end': name_span[1]}:
                occurrence['location'] = contract.profile.location(source, name_span[1] + 2, name_span[1] + 9)
                break
        else:
            self.fail('trace did not disclose its parameter name')
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

    def test_initialization_location_must_name_a_whole_module_statement(self):
        value, row = self.changed('default-evaluated-override/moved')
        # Point to the nested source call rather than the surrounding declaration.
        source = row['case']['source']
        tree = contract.ast.parse(source)
        call = next(node for node in contract.ast.walk(tree)
                    if isinstance(node, contract.ast.Call)
                    and isinstance(node.func, contract.ast.Name)
                    and node.func.id == row['case']['names']['source'])
        lines = source.encode().splitlines(keepends=True)
        start = sum(map(len, lines[:call.lineno - 1])) + call.col_offset
        end = sum(map(len, lines[:call.end_lineno - 1])) + call.end_col_offset
        row['flow']['initialization']['sites'][0]['occurrence']['location'] = contract.profile.location(source, start, end)
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

    def test_equal_sink_offsets_do_not_merge_distinct_files(self):
        value, row = self.changed('import-distinct-sink-files/moved')
        self.assertEqual(len(row['flow']['witnesses']), 2)
        row['flow']['witnesses'][1] = copy.deepcopy(row['flow']['witnesses'][0])
        self.reject(value)

    def test_imported_sink_endpoint_and_caller_suffix_are_checked(self):
        for mutation in ('endpoint', 'suffix-role', 'suffix-location'):
            value, row = self.changed('import-helper-sink/moved')
            witness = row['flow']['witnesses'][0]
            points = witness['trace']['occurrences']
            if mutation == 'endpoint':
                witness['site'] = copy.deepcopy(points[-1])
            elif mutation == 'suffix-role':
                points[-1]['role'] = 'use'
            else:
                points[-1]['location'] = copy.deepcopy(points[1]['location'])
                points[-1]['path'] = points[1]['path']
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

    def test_refusal_cannot_become_an_incomplete_report(self):
        value, row = self.changed('refuse-entry-syntax/moved')
        row['flow'] = {'complete': False, 'witnesses': []}
        self.reject(value)

    def test_refusal_exit_status_and_diagnostic(self):
        for field, replacement in [('exit_code', 0), ('exit_code', True), ('stderr', '')]:
            value, row = self.changed('refuse-import-entry/moved')
            row['invocations'][1][field] = replacement
            self.reject(value)
        value, row = self.changed('refuse-entry-syntax/moved')
        row['error']['error']['kind'] = 'refused'
        self.reject(value)

    def test_report_cannot_ignore_failed_invocation(self):
        value, row = self.changed()
        row['invocations'][1]['exit_code'] = 1
        self.reject(value)

    def test_depth_budget_is_bound_to_command_and_report(self):
        for mutation in ('command', 'report'):
            value, row = self.changed('discovery-depth-2-selected/moved')
            if mutation == 'command':
                command = row['commands'][1]
                command[command.index('--depth') + 1] = '3'
            else:
                row['flow']['inputs']['budget']['depth'] = 3
                row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
            self.reject(value)

    def test_module_budget_cannot_admit_unloaded_source(self):
        value, row = self.changed('import-module-limit-17-selected/moved')
        path = 'renamed_unit16.py'
        row['flow']['inputs']['modules']['files'][path] = contract.profile.compiler.digest(row['case']['files'][path])
        row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
        self.reject(value)

    def test_stub_candidate_cannot_disappear(self):
        value, row = self.changed('import-ambiguous-stub-selected/moved')
        candidate = row['flow']['inputs']['modules']['lookups'][0]['candidates']['engine.pyi']
        candidate.clear()
        candidate['status'] = 'missing'
        row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
        self.reject(value)

    def test_binding_budget_cannot_claim_a_resolved_function(self):
        value, row = self.changed('import-binding-limit-selected/moved')
        lookup = next(item for item in row['flow']['inputs']['modules']['lookups']
                      if item['importer'] == row['case']['path'])
        lookup['resolution'] = 'function'
        row['flow']['input_digest'] = contract.profile.compiler.digest(row['flow']['inputs'])
        self.reject(value)

    def test_summary_cannot_overstate_coverage(self):
        value, _ = self.changed()
        value['summary']['cases'] += 2
        self.reject(value)


if __name__ == '__main__':
    unittest.main()
