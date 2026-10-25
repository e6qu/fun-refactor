"""Keep local native attempts bounded, independent and honest about missing costs."""
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_local as local
from agent_eval.selected_cli import verify_export


class SelectedLocalTests(unittest.TestCase):
    def test_native_invocations_choose_exact_models_and_new_conversations(self):
        profiles = json.loads((local.ROOT / local.DIRECTORY / 'profiles.json').read_text())
        binaries = {'codex': {'path': '/codex'}, 'opencode2': {'path': '/opencode2'}}
        for name, profile in profiles.items():
            argv, stdin = local.invocation(profile, binaries, Path('/project'), Path('/output'), 'the request', 'workspace-write')
            self.assertNotIn('--continue', argv)
            self.assertNotIn('--resume', argv)
            self.assertNotIn('--dangerously-bypass-approvals-and-sandbox', argv)
            if name == 'codex-luna-fast':
                self.assertIn('service_tier="fast"', argv)
                self.assertIn('model_reasoning_effort="high"', argv)
                self.assertIn('features.multi_agent=false', argv)
                self.assertEqual(stdin, b'the request')
            else:
                self.assertIn('--standalone', argv)
                self.assertTrue(argv[argv.index('--session') + 1].startswith('ses_'))
                self.assertEqual(argv[argv.index('--model') + 1],
                    profile['model']['providerID'] + '/' + profile['model']['id'] + '#high')
                config = local.configuration(profile, 'files', Path('/project'))
                self.assertEqual(config['agents']['build']['model'], config['model'])
                self.assertEqual(config['skills'], [])
                self.assertEqual(config['permissions'][0], {'action': '*', 'resource': '*', 'effect': 'deny'})

    def test_cli_accounting_does_not_invent_free_billing_or_completion(self):
        self.assertEqual(local.usage('codex', [])['complete_turns'], 0)
        self.assertIsNone(local.usage('codex', [])['actual_cost'])
        reported = local.usage('opencode2', [{'type': 'step_finish', 'part': {'tokens': {'input': 3}, 'cost': 0}}])
        self.assertEqual(reported['reported'][0]['catalog_cost'], 0)
        self.assertIsNone(reported['actual_cost'])

    def test_untrusted_archive_cannot_write_outside_the_fresh_project(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'source.tar.gz'
            with tarfile.open(archive, 'w:gz') as stream:
                entry = tarfile.TarInfo('../outside')
                entry.size = 4
                stream.addfile(entry, io.BytesIO(b'data'))
            receipt = {'archive_sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
                       'original': {'../outside': {'sha256': '', 'mode': 0o644}}}
            with self.assertRaisesRegex(ValueError, 'unsafe or repeated'):
                local.unpack(archive, receipt, root / 'project')
            self.assertFalse((root / 'outside').exists())

    def test_task_prompts_keep_the_property_fixed_without_supplied_tactics(self):
        for setup in ('files', 'fr'):
            prompt = local.prompt('access-proof', setup, {name: {'path': '/' + name} for name in ('python', 'cargo', 'lake')})
            self.assertIn(local.tasks.TASKS['access-proof']['request'], prompt)
            self.assertNotIn('rfl', prompt)
            self.assertIn('change.patch', prompt)
            self.assertIn('Do not start a nested guard', prompt)

    def test_v2_export_pins_model_variant_and_agrees_with_stream_usage(self):
        profile = {'model': {'providerID': 'kimi-code-plan-global', 'id': 'k3', 'variant': 'high'}}
        saved = {'info': {'id': 'ses_example', 'agent': 'build', 'model': profile['model'], 'outcome': 'succeeded'},
                 'messages': [{'type': 'agent-switched', 'agent': 'build'},
                     {'id': 'msg_example', 'type': 'assistant', 'agent': 'build', 'model': profile['model'],
                      'tokens': {'input': 2, 'output': 1}, 'cost': 0}]}
        events = [{'type': 'step_finish', 'part': {'messageID': 'msg_example', 'tokens': {'input': 2, 'output': 1}, 'cost': 0}}]
        result = verify_export(saved, profile, 'ses_example', events)
        self.assertTrue(result['model_observed'])
        saved['messages'][1]['model'] = {**profile['model'], 'variant': 'low'}
        with self.assertRaisesRegex(ValueError, 'substituted'):
            verify_export(saved, profile, 'ses_example', events)
        saved['messages'][1]['model'] = profile['model']
        events[0]['part']['tokens']['input'] = 0
        with self.assertRaisesRegex(ValueError, 'usage differ'):
            verify_export(saved, profile, 'ses_example', events)


if __name__ == '__main__':
    unittest.main()
