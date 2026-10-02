"""Offline controls for direct reads and exact references to disclosed source."""
import base64
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_references as refs, native_mcp as mcp, opencode_native as native, source_coverage
from agent_eval import native_discovery
from agent_eval.source_disclosure import read_source
from agent_eval.study import digest, encode, load
from agent_eval.workspace_bundle import unpack

ROOT = Path(__file__).resolve().parents[2]


class Contract(unittest.TestCase):
    def setUp(self):
        self.raw = ('header\névidence: exact source with required anchor\n' + 'é' * 2200 + '\n').encode()
        self.files = {'example.py': {'data': base64.b64encode(self.raw).decode(), 'executable': False}}
        self.args = {'path': 'example.py', 'offset': 7, 'bytes': 8192, 'sha256': ''}

    def test_first_offset_and_stale_identity(self):
        page = refs.read_frozen(self.files, self.args, 16384)
        self.assertTrue(page['text'].startswith('évidence:'))
        self.assertEqual(page['offset'], 7)
        for changes, error in (({'sha256': '0' * 64}, 'stale_source'),
                               ({'offset': 8}, 'invalid_utf8_or_boundary'),
                               ({'offset': len(self.raw) + 1}, 'offset_out_of_range'),
                               ({'path': 'missing.py'}, 'missing_file')):
            self.assertEqual(refs.read_frozen(self.files, {**self.args, **changes}, 16384)['error'], error)
        with self.assertRaises(ValueError):
            read_source(self.files, self.args, 16384)

    def test_reference_exact_source_and_bounds(self):
        request = {'action': 'read', **self.args}
        out = refs.action(self.files, request, 'files', Path('fr'), Path('.'), None)
        spans = native_discovery.disclosed(self.files, request, out)
        pieces = refs.pieces(spans)
        self.assertEqual(len(pieces), 3)
        self.assertEqual([x['source'] for x in out['source_refs']], list(pieces))
        for part in pieces.values():
            self.assertTrue(16 <= len(part['text'].encode()) <= 2048)
            self.assertEqual(self.raw[part['start']:part['end']], part['text'].encode())
        answer = {'claim': {'value': True, 'citations': [{'source': next(iter(pieces))}]}}
        resolved = refs.resolve(answer, spans)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / 'source'
            unpack(self.files, root, 1024**2)
            payload = {'answer': resolved, 'disclosed': spans,
                       'criteria': {'claim': {'value': True, 'evidence': [[{'path': 'example.py', 'contains': 'required anchor'}]]}}}
            self.assertTrue(native.grade(root, payload, source_coverage.POLICY)['passed'])
            missing_anchor = {'claim': {'value': True, 'citations': [{'source': list(pieces)[1]}]}}
            self.assertFalse(native.grade(root, {**payload, 'answer': missing_anchor}, source_coverage.POLICY,
                                          references=True)['passed'])
            payload['answer']['claim']['value'] = False
            self.assertFalse(native.grade(root, payload, source_coverage.POLICY)['passed'])
        for unavailable in ([], [{**spans[0], 'sha256': '0' * 64}], [{**spans[0], 'path': 'other.py'}]):
            with self.assertRaises(ValueError):
                refs.resolve(answer, unavailable)

    def test_forged_refs_and_protocols_refuse(self):
        request = {'action': 'read', **self.args}
        page = refs.read_frozen(self.files, self.args, 16384)
        with self.assertRaises(ValueError):
            refs.decorate(self.files, request, {**page, 'source_refs': []})
        with self.assertRaises(ValueError):
            refs.resolve({'x': {'value': True, 'citations': [{'source': 'invented', 'quote': 'bad'}]}}, [])
        with self.assertRaises(ValueError):
            refs.read_frozen(self.files, {**self.args, 'path': '../secret'}, 16384)

    def test_metadata_is_not_source_and_fr_source_is_independently_checked(self):
        from agent_eval.test_opencode_native import FILES
        handle = 'frp1:' + 'a' * 32 + ':1'
        names = {'mode': 'names', 'profile': {'name': 'compact', 'source_bytes': 2048}, 'rows': []}
        request = {'action': 'fr', 'operation': 'explore', 'term': 'value'}
        self.assertNotIn('source_refs', refs.action(FILES, request, 'fr', Path('fr'), Path('.'), lambda *_: encode(names)))
        behavior = {**names, 'mode': 'behavior', 'declaration': {
            'node': {'handle': handle, 'path': 'module.py', 'span': {'start': 0, 'end': 27}},
            'source': {'offset': 0, 'returned_bytes': 27, 'span': {'start': 0, 'end': 27},
                       'text': 'def value():\n    return 42\n'}}}
        request.update(mode='behavior', target=handle)
        result = refs.action(FILES, request, 'fr', Path('fr'), Path('.'), lambda *_: encode(behavior))
        read = refs.action(FILES, {'action': 'read', 'path': 'module.py', 'offset': 0, 'bytes': 8192, 'sha256': ''},
                           'files', Path('fr'), Path('.'), None)
        self.assertEqual(result['source_refs'], read['source_refs'])
        behavior['declaration']['source']['text'] = 'def value():\n    return 43\n'
        with self.assertRaisesRegex(ValueError, 'snapshot'):
            refs.action(FILES, request, 'fr', Path('fr'), Path('.'), lambda *_: encode(behavior))

    def test_reference_coverage_and_resource_limits(self):
        span = {'path': 'source.py', 'sha256': 'a' * 64, 'start': 0, 'end': 16, 'text': 'x' * 16}
        self.assertEqual(refs.pieces([span, span]), refs.pieces([span]))
        self.assertFalse(refs.pieces([{**span, 'end': 15, 'text': 'x' * 15}]))
        for spans in ([span, {**span, 'text': 'y' * 16}], [span] * 1025,
                      [{**span, 'text': 'x' * 2048, 'end': 2048}] * 513):
            with self.assertRaises(ValueError):
                refs.pieces(spans)


class NativeProtocol(unittest.TestCase):
    def test_retained_pilot_replays_offline_with_failures_and_overhead(self):
        root = ROOT / 'tests/agent-eval/opencode/results/2026-10-02-source-references'
        with patch.object(native, 'bounded_run', side_effect=AssertionError('replay must stay offline')):
            report = native.report(load(root / 'plan.json'), root / 'attempts')
        self.assertEqual(report, load(root / 'report.json'))
        self.assertEqual((report['planned'], report['passed']), (6, 2))
        self.assertEqual(sum(row['status'] == 'failed' for row in report['attempts']), 4)
        complete = [row['audit']['metrics'] for row in report['attempts'] if row['status'] == 'completed']
        self.assertEqual(sorted(row['fr_requests'] for row in complete), [0, 3])
        self.assertTrue(all(row['source_reference_citations'] == 5 for row in complete))
        self.assertTrue(all(row['source_reference_metadata_bytes'] > 0 for row in complete))

    def transcript(self, same_step=False):
        from agent_eval.test_opencode_native import fixture, FILES
        events, exported, rows = fixture(same_step)
        parts = [part for message in exported['messages'] for part in message['parts'] if part['type'] == 'tool']
        request = {'action': 'read', **rows[0]['params']['arguments']}
        result = refs.action(FILES, request, 'files', Path('fr'), Path('.'), None)
        answer = {'value': {'value': 42, 'citations': [{'source': result['source_refs'][0]['source']}]}}
        rows[0]['result'] = result
        rows[1]['params']['arguments'] = {'answer': answer}
        for row, part in zip(rows, parts):
            text = encode(row['result']).decode()
            row['response'] = {'content': [{'type': 'text', 'text': text}], 'isError': False}
            part['state']['input'] = row['params']['arguments']
            part['state']['output'] = text
        exported['messages'].insert(0, {'info': {'role': 'user'}, 'parts': [{'type': 'text', 'text': refs.PROMPT + '\nTask:\ntask'}]})
        task = {'files': FILES, 'requirement': 'task'}
        plan = {'tools_schema_version': 4, 'prompt': refs.PROMPT, 'tools': {'files': refs.schemas('files')}}
        return events, exported, rows, task, plan

    def audit(self, data):
        events, exported, rows, task, plan = data
        return native.audit(b'\n'.join(encode(event) for event in events), exported, rows, task,
                            {'arm': 'files', 'model': 'provider/model'}, plan)

    def grade(self, result):
        from agent_eval.test_opencode_native import FILES
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'source'
            unpack(FILES, root, 1024**2)
            return native.grade(root, {'answer': result['answer'], 'disclosed': result['disclosed'],
                                      'criteria': {'value': {'value': 42, 'evidence': [[{'path': 'module.py', 'contains': 'return 42'}]]}}},
                                source_coverage.POLICY, references=True)

    def test_reference_submission_requires_prior_assistant_step(self):
        before = self.audit(self.transcript())
        same = self.audit(self.transcript(same_step=True))
        self.assertTrue(self.grade(before)['passed'])
        self.assertFalse(self.grade(same)['passed'])
        self.assertEqual(before['metrics']['source_reference_citations'], 1)
        self.assertGreater(before['metrics']['source_reference_metadata_bytes'], 0)
        self.assertEqual(before['metrics']['source_reference_metadata_bytes'], same['metrics']['source_reference_metadata_bytes'])
        self.assertEqual(before['metrics']['unique_source_bytes'], 27)
        self.assertEqual(same['disclosed'], [])
        before['answer']['value']['value'] = 43
        self.assertFalse(self.grade(before)['passed'])

    def test_forged_reference_metadata_cannot_replay(self):
        data = self.transcript()
        row = data[2][0]
        row['result']['source_refs'][0]['source'] = 'src1:' + '0' * 64
        text = encode(row['result']).decode()
        row['response']['content'][0]['text'] = text
        data[1]['messages'][1]['parts'][0]['state']['output'] = text
        with self.assertRaisesRegex(ValueError, 'replay'):
            self.audit(data)

    def test_malformed_submission_refuses_before_metric_accounting(self):
        data = self.transcript()
        data[2][1]['params']['arguments']['answer'] = 'not an object'
        with self.assertRaisesRegex(ValueError, 'argument type'):
            self.audit(data)

    def test_nonzero_first_read_is_versioned_and_stdio_returns_references(self):
        from agent_eval.test_opencode_native import config, exchange
        arguments = {'path': 'module.py', 'offset': 4, 'bytes': 8192, 'sha256': ''}
        for version in (2, 3, 4):
            server = mcp.Server({**config(), 'tools_schema_version': version}, io.BytesIO())
            result = server.call({'name': 'read_source', 'arguments': arguments})
            self.assertEqual(result['isError'], version != 4)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'config').write_bytes(encode({**config(), 'tools_schema_version': 4}))
            requests = [exchange('initialize', {'protocolVersion': mcp.PROTOCOL}),
                        exchange('notifications/initialized', identity=None),
                        exchange('tools/call', {'name': 'read_source', 'arguments': arguments}, 2)]
            process = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/native-rehearsal.py'), 'serve',
                                      str(root / 'config'), str(root / 'log')],
                                     input=b''.join(encode(row) + b'\n' for row in requests), capture_output=True, timeout=5)
            self.assertEqual(process.returncode, 0, process.stderr)
            response = json.loads(process.stdout.splitlines()[-1])['result']
            page = json.loads(response['content'][0]['text'])
            self.assertEqual(page['offset'], 4)
            self.assertTrue(page['source_refs'])
            self.assertEqual(page['source_refs'][0]['start'], 4)

    def test_new_freeze_is_opt_in_balanced_and_rejects_rehashed_protocol_changes(self):
        manifest = ROOT / 'tests/agent-eval/opencode/explanations.json'
        for guide in (None, ROOT / 'skills/fr/references/explore.md'):
            old = native.freeze(load(manifest), manifest.parent, Path(__file__), guide)
            self.assertEqual(old['plan']['tools_schema_version'], 3 if guide else 2)
            frozen = native.freeze(load(manifest), manifest.parent, Path(__file__), guide, source_references=True)
            self.assertEqual(frozen['plan']['tools_schema_version'], 4)
            self.assertEqual(len(native.checked(frozen)['cells']), 18 if guide else 12)
            for mutation in ('prompt', 'tools', 'cells', 'source_policy'):
                changed = copy.deepcopy(frozen)
                plan = changed['plan']
                if mutation == 'prompt':
                    plan['prompt'] += ' invented'
                elif mutation == 'tools':
                    plan['tools']['files'][2]['inputSchema']['properties']['bytes']['maximum'] += 1
                elif mutation == 'source_policy':
                    plan['source_policy'] = None
                else:
                    plan['source_plan']['plan']['cells'].reverse()
                    plan['source_plan']['sha256'] = digest(plan['source_plan']['plan'])
                changed['sha256'] = digest(plan)
                with self.assertRaises(ValueError):
                    native.checked(changed)


if __name__ == '__main__':
    unittest.main()
