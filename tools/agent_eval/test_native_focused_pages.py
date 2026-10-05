"""Focused CLI pages stay bounded, source-checked and opt-in for native trials."""
import copy
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_costs, native_mcp, native_references, opencode_native
from agent_eval.study import digest, encode, load
from agent_eval.test_opencode_native import FILES, config

ROOT = Path(__file__).resolve().parents[2]
HANDLE = 'frp1:' + 'a' * 32 + ':1'


def response(view):
    result = {'mode': 'behavior', 'view': view,
              'profile': {'name': 'compact', 'source_bytes': 2048},
              'declaration': {'node': {'handle': HANDLE, 'path': 'module.py',
                                       'span': {'start': 0, 'end': 27}}}}
    if view != 'relationships':
        result['declaration']['source'] = {
            'offset': 0, 'returned_bytes': 27, 'span': {'start': 0, 'end': 27},
            'text': 'def value():\n    return 42\n'}
    if view != 'source':
        result['relationships'] = {'items': [], 'page': {'next': None}}
    return result


class FocusedPages(unittest.TestCase):
    def test_completed_audit_counts_recovery_and_only_actual_source_reads(self):
        from agent_eval.test_source_reviews import transcript
        server_type = native_mcp.Server
        def current(config, log):
            return server_type({**config, 'tools_schema_version': 6}, log)
        plan = {'tools_schema_version': 6, 'prompt': native_references.PROMPT,
                'tools': {'fr': native_references.recovery_schemas('fr')}}
        task = {'files': FILES, 'requirement': 'task'}
        read = {'name': 'read_source', 'arguments': {
            'path': 'module.py', 'offset': 0, 'bytes': 8192, 'sha256': ''}}
        source = native_references.read_frozen(FILES, read['arguments'], 16384)
        decorated = native_references.decorate(FILES, {'action': 'read', **read['arguments']}, source)
        answer = {'value': {'value': 42, 'citations': [{'source': decorated['source_refs'][0]['source']}]}}
        calls = [[{'name': 'fr_explore', 'arguments': {'term': 'value', 'mode': 'behavior'}}],
                 [read], [{'name': 'submit_answer', 'arguments': {'answer': answer}}], []]
        with patch.object(native_mcp, 'Server', side_effect=current):
            raw, exported, log = transcript(plan, task, answer, calls)
        events = [json.loads(row) for row in raw.splitlines()]
        for part in [event['part'] for event in events if event['type'] == 'tool_use'] + [
            part for message in exported['messages'] for part in message['parts'] if part['type'] == 'tool'
        ]:
            if part['tool'] == 'rehearsal_fr_explore':
                part['state']['status'] = 'error'
                part['state']['error'] = part['state'].pop('output')
        raw = b'\n'.join(encode(event) for event in events)
        rows = [json.loads(row) for row in log.splitlines()]
        self.assertIn('next', rows[0]['result'])
        result = opencode_native.audit(raw, exported, rows, task, {'arm': 'fr', 'model': 'provider/model'}, plan)
        self.assertEqual(result['metrics']['tool_calls'], 3)
        self.assertEqual(result['metrics']['source_available_before_answer_bytes'], 27)
        self.assertEqual(result['metrics']['source_reference_citations'], 1)
        self.assertEqual(len(result['disclosed']), 1)

    def test_recovery_is_opt_in_preserves_scope_and_does_not_execute_a_search(self):
        request = {'name': 'fr_explore', 'arguments': {
            'term': 'val', 'mode': 'behavior', 'target': 'wrong', 'path': 'module.py',
            'contains': True, 'view': 'source', 'offset': 12}}
        for version in (4, 5, 6):
            server = self.server('source', version)
            actual = copy.deepcopy(request)
            if version == 4:
                del actual['arguments']['view']
            result = server.call(actual)
            self.assertTrue(result['isError'])
            self.assertEqual(self.executed, [])
            value = json.loads(result['content'][0]['text'])
            self.assertEqual(value['error'], 'behavior needs a full fr handle')
            if version < 6:
                self.assertEqual(set(value), {'error'})
                continue
            self.assertEqual(value['next'], {'name': 'fr_explore', 'arguments': {
                'term': 'val', 'path': 'module.py', 'contains': True, 'mode': 'names'}})
            self.assertNotIn('source_refs', value)
            native_mcp.arguments(server.tools['fr_explore']['inputSchema'], value['next']['arguments'])
            server.execute = lambda args, *_: self.executed.append(args) or encode({
                'mode': 'names', 'profile': {'name': 'compact', 'source_bytes': 2048}, 'rows': []})
            self.assertFalse(server.call(value['next'])['isError'])
            self.assertEqual(len(self.executed), 1)
            self.assertIn('--contains', self.executed[0])
            self.assertEqual(self.executed[0][-3:], ['--in', 'module.py', '--contains'])

    def test_recovery_does_not_repair_unrelated_invalid_requests(self):
        for arguments in (
            {'term': 'value', 'mode': 'behavior', 'path': 'missing.py'},
            {'term': '-flag', 'mode': 'behavior'},
            {'term': 'value', 'mode': 'behavior', 'contains': 'true'},
            {'term': 'value', 'mode': 'names', 'offset': 12},
            {'term': 'value', 'mode': 'behavior', 'target': HANDLE, 'view': 'unknown'},
        ):
            server = self.server('both', 6)
            result = server.call({'name': 'fr_explore', 'arguments': arguments})
            self.assertTrue(result['isError'])
            self.assertNotIn('next', json.loads(result['content'][0]['text']))
            self.assertEqual(self.executed, [])
        ordinary = native_mcp.Server({**config('files'), 'tools_schema_version': 6}, io.BytesIO())
        refused = ordinary.call({'name': 'fr_explore', 'arguments': {'term': 'value', 'mode': 'behavior'}})
        self.assertNotIn('next', json.loads(refused['content'][0]['text']))

    def test_recovery_freeze_preserves_limits_and_rejects_version_downgrade(self):
        manifest = ROOT / 'tests/agent-eval/opencode/explanations.json'
        args = (load(manifest), manifest.parent, Path(__file__))
        plan = opencode_native.freeze_recovery(*args)
        self.assertEqual(plan['plan']['tools_schema_version'], 6)
        self.assertEqual(plan['plan']['limits'], opencode_native.LIMITS)
        self.assertEqual(len(opencode_native.checked(plan)['cells']), 12)
        self.assertEqual(native_references.recovery_schemas('files'), native_references.schemas('files'))
        altered = copy.deepcopy(plan)
        altered['plan']['tools_schema_version'] = 5
        altered['sha256'] = digest(altered['plan'])
        with self.assertRaises(ValueError):
            opencode_native.checked(altered)

    def test_recovery_results_replay_without_becoming_source_evidence(self):
        log = io.BytesIO()
        server = self.server('both', 6, log)
        out = server.call({'name': 'fr_explore', 'arguments': {'term': 'value', 'mode': 'behavior'}})
        self.assertTrue(out['isError'])
        raw = log.getvalue()
        observed = native_costs.observed(b'', raw, {'files': FILES}, 'fr', 6, read_only=True)['observed']
        self.assertEqual((observed['host_calls'], observed['refused_calls'], observed['source_page_bytes']), (1, 1, 0))
        row = json.loads(raw)
        row['result']['next']['arguments']['term'] = 'forged'
        row['response']['content'][0]['text'] = encode(row['result']).decode()
        with self.assertRaises(ValueError):
            native_costs.observed(b'', encode(row) + b'\n', {'files': FILES}, 'fr', 6, read_only=True)

    def server(self, view, version=5, log=None):
        self.executed = []
        def execute(args, *_):
            self.executed.append(args)
            return encode(response(view))
        return native_mcp.Server({**config('fr'), 'tools_schema_version': version},
                                 log if log is not None else io.BytesIO(), execute)

    def call(self, server, view):
        return server.call({'name': 'fr_explore', 'arguments': {
            'term': 'value', 'mode': 'behavior', 'target': HANDLE, 'view': view}})

    def test_schema_is_opt_in_and_the_server_forwards_the_exact_view(self):
        for view in ('source', 'relationships'):
            old = self.call(self.server(view, 4), view)
            self.assertTrue(old['isError'])
            self.assertEqual(self.executed, [])
            current = self.call(self.server(view), view)
            self.assertFalse(current['isError'], current)
            args = self.executed[0]
            self.assertEqual(args[args.index('--view') + 1], view)
            self.assertEqual(args[args.index('--profile') + 1], 'compact')
            result = json.loads(current['content'][0]['text'])
            self.assertEqual('source_refs' in result, view == 'source')
            if view == 'source':
                self.assertEqual(result['source_refs'][0]['start'], 0)
                self.assertEqual(result['source_refs'][0]['end'], 27)
                self.assertNotIn('relationships', result)
            else:
                self.assertNotIn('source', result['declaration'])

    def test_unchanged_source_reference_schema_stays_identical_to_the_retained_plan(self):
        plan = load(ROOT / 'tests/agent-eval/opencode/results/2026-10-02-source-references/plan.json')['plan']
        self.assertEqual(plan['tools'], {arm: native_references.schemas(arm) for arm in plan['tools']})
        self.assertEqual(native_references.schemas('files'),
                         native_references.schemas('files', focused_pages=True))

    def test_legacy_both_view_still_accepts_retained_reports_without_a_view_field(self):
        result = response('both')
        del result['view']
        server = native_mcp.Server({**config('fr'), 'tools_schema_version': 4}, io.BytesIO(),
                                   lambda *_: encode(result))
        out = server.call({'name': 'fr_explore', 'arguments': {
            'term': 'value', 'mode': 'behavior', 'target': HANDLE}})
        self.assertFalse(out['isError'], out)

    def test_missing_or_contradictory_view_and_forged_source_refuse(self):
        request = {'action': 'fr', 'operation': 'explore', 'term': 'value',
                   'mode': 'behavior', 'target': HANDLE, 'view': 'source'}
        broken = [response('both'), response('relationships')]
        missing = response('source')
        del missing['view']
        broken.append(missing)
        excess = response('source')
        excess['relationships'] = {'items': []}
        broken.append(excess)
        forged = response('source')
        forged['declaration']['source']['text'] = 'def value():\n    return 43\n'
        broken.append(forged)
        for result in broken:
            with self.assertRaises(ValueError):
                native_references.action(FILES, request, 'fr', Path('fr'), Path('.'), lambda *_: encode(result))
        result = response('relationships')
        result['declaration']['source'] = response('source')['declaration']['source']
        with self.assertRaises(ValueError):
            native_references.action(FILES, {**request, 'view': 'relationships'}, 'fr',
                                     Path('fr'), Path('.'), lambda *_: encode(result))

    def test_invalid_combinations_refuse_before_executing_fr(self):
        base = {'term': 'value', 'mode': 'behavior', 'target': HANDLE}
        for args in (
            {**base, 'view': 'source', 'relations_cursor': 'cursor'},
            {**base, 'view': 'relationships', 'offset': 1},
            {'term': 'value', 'mode': 'names', 'view': 'source'},
            {**base, 'view': 'invented'},
        ):
            server = self.server('both')
            out = server.call({'name': 'fr_explore', 'arguments': args})
            self.assertTrue(out['isError'], out)
            self.assertEqual(self.executed, [])

    def test_partial_replay_counts_source_only_when_it_was_produced(self):
        log = io.BytesIO()
        server = self.server('source', log=log)
        self.assertFalse(self.call(server, 'source')['isError'])
        server.execute = lambda *_: encode(response('relationships'))
        self.assertFalse(self.call(server, 'relationships')['isError'])
        result = native_costs.observed(b'', log.getvalue(), {'files': FILES}, 'fr', 5, read_only=True)['observed']
        self.assertEqual(result['host_calls'], 2)
        self.assertEqual(result['source_page_bytes'], 27)
        self.assertEqual(result['native_confirmed_source_page_bytes'], 0)
        self.assertEqual(result['native_confirmed_results'], 0)

    def test_new_freeze_requires_source_references_and_preserves_old_defaults(self):
        manifest = ROOT / 'tests/agent-eval/opencode/explanations.json'
        args = (load(manifest), manifest.parent, Path(__file__))
        with self.assertRaises(ValueError):
            opencode_native.freeze(*args, focused_pages=True)
        ordinary = opencode_native.freeze(*args, source_references=True)
        self.assertEqual(ordinary['plan']['tools_schema_version'], 4)
        focused = opencode_native.freeze(*args, source_references=True, focused_pages=True)
        self.assertEqual(focused['plan']['tools_schema_version'], 5)
        self.assertEqual(len(opencode_native.checked(focused)['cells']), 12)
        changed = copy.deepcopy(focused)
        changed['plan']['tools_schema_version'] = 4
        changed['sha256'] = digest(changed['plan'])
        with self.assertRaises(ValueError):
            opencode_native.checked(changed)


    def test_completed_source_reference_audit_supports_the_new_version(self):
        from agent_eval.test_native_references import NativeProtocol
        control = NativeProtocol('test_reference_submission_requires_prior_assistant_step')
        data = control.transcript()
        data[4]['tools_schema_version'] = 5
        result = control.audit(data)
        self.assertEqual(result['metrics']['source_reference_citations'], 1)
        self.assertTrue(control.grade(result)['passed'])


if __name__ == '__main__':
    unittest.main()
