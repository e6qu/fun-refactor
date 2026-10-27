"""Keep local native attempts bounded, independent and honest about missing costs."""
import hashlib
import io
import json
import shlex
import subprocess
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_local as local
from agent_eval.selected_cli import verify_export


class SelectedLocalTests(unittest.TestCase):
    def test_expired_attempt_keeps_its_failure_and_partial_usage_without_an_export(self):
        events = [{'type': 'step_finish', 'part': {'tokens': {'input': 3}, 'cost': 0}}]
        report = {'status': 'failed', 'usage': local.usage('opencode2', events),
                  'process': {'timed_out': True, 'stop_reason': 'wall_seconds'}, 'error': None}
        with mock.patch.object(local.time, 'monotonic', return_value=150.03), \
             mock.patch.object(local, 'export_session') as export:
            local.record_opencode_accounting(report, '/client', 'ses_example', Path('/project'), {},
                Path('/output'), {}, events, deadline=150)
        export.assert_not_called()
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['session_accounting'], {'status': 'deadline-exceeded', 'exported': False})
        self.assertEqual(report['process'], {'timed_out': True, 'stop_reason': 'wall_seconds'})
        self.assertEqual(report['usage']['reported'], [{'tokens': {'input': 3}, 'catalog_cost': 0}])
        self.assertIsNone(report['usage']['exported'])
        self.assertIsNone(report['usage']['actual_cost'])
        self.assertIsNone(report['error'])

    def test_session_export_timeout_is_a_failed_attempt_with_unknown_complete_usage(self):
        report = {'status': 'completed', 'usage': local.usage('opencode2', []), 'error': None}
        with mock.patch.object(local.time, 'monotonic', return_value=148.75), \
             mock.patch.object(local, 'export_session', side_effect=subprocess.TimeoutExpired('/client', 1.25)) as export:
            local.record_opencode_accounting(report, '/client', 'ses_example', Path('/project'), {},
                Path('/output'), {}, [], deadline=150)
        self.assertEqual(export.call_args.kwargs['timeout_seconds'], 1.25)
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['session_accounting'], {'status': 'timed-out', 'exported': False})
        self.assertIsNone(report['usage']['exported'])
        self.assertIsNone(report['error'])

    def test_accounting_does_not_accept_another_model_or_promote_a_timed_out_client(self):
        profile = {'model': {'providerID': 'kimi-code-plan-global', 'id': 'k3', 'variant': 'high'}}
        saved = {'info': {'id': 'ses_example', 'agent': 'build', 'model': profile['model'], 'outcome': 'succeeded'},
                 'messages': [{'id': 'msg_example', 'type': 'assistant', 'agent': 'build', 'model': profile['model'],
                    'finish': 'stop', 'tokens': {'input': 2}, 'cost': 0}]}
        events = [{'type': 'step_finish', 'part': {'messageID': 'msg_example', 'tokens': {'input': 2}, 'cost': 0}}]
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            report = {'status': 'failed', 'usage': local.usage('opencode2', events)}
            with mock.patch.object(local.time, 'monotonic', return_value=140), \
                 mock.patch.object(local, 'export_session', return_value=saved) as export:
                local.record_opencode_accounting(report, '/client', 'ses_example', Path('/project'), {},
                    output, profile, events, deadline=150)
            self.assertEqual(export.call_args.kwargs['timeout_seconds'], 10)
            self.assertEqual(report['status'], 'failed')
            self.assertTrue(report['usage']['exported']['model_observed'])
            self.assertEqual(json.loads((output / 'session-export.json').read_text()), saved)
            saved['info']['model'] = {**profile['model'], 'id': 'another-model'}
            with mock.patch.object(local.time, 'monotonic', return_value=140), \
                 mock.patch.object(local, 'export_session', return_value=saved), \
                 self.assertRaisesRegex(ValueError, 'model or agent differs'):
                local.record_opencode_accounting(report, '/client', 'ses_example', Path('/project'), {},
                    output, profile, events, deadline=150)

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

    def test_oversized_patch_keeps_failed_source_and_removes_compiler_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project, output = root / 'project', root / 'record'
            project.mkdir()
            output.mkdir()
            source = 'def invoice_total(amount, fee):\n    return amount + fee  # café\n'.encode()
            (project / 'invoice.py').write_bytes(source)
            (project / 'change.patch').write_bytes(b'x' * (1024**2 + 1))
            (project / 'target').mkdir()
            (project / 'target/generated').write_bytes(b'compiler output')
            record = {'status': 'completed', 'patch_delivered': False, 'patch_sha256': None, 'collection_errors': []}
            local.retain_result(project, output, record)
            self.assertEqual(record['status'], 'failed')
            self.assertEqual(record['collection_errors'][0]['stage'], 'patch')
            self.assertFalse(record['patch_delivered'])
            self.assertFalse((output / 'change.patch').exists())
            self.assertFalse((project / 'target').exists())
            with tarfile.open(output / 'candidate.tar.gz') as archive:
                self.assertEqual(archive.getnames(), ['invoice.py'])
                self.assertEqual(archive.extractfile('invoice.py').read(), source)

    def test_capture_failure_still_unlinks_a_target_symlink_without_following_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project, output, unrelated = root / 'project', root / 'record', root / 'unrelated'
            for path in (project, output, unrelated):
                path.mkdir()
            keep = unrelated / 'keep'
            keep.write_text('preserve unrelated data')
            (project / 'target').symlink_to(unrelated, target_is_directory=True)
            (project / 'source.py').symlink_to(keep)
            record = {'status': 'completed', 'patch_delivered': False, 'patch_sha256': None, 'collection_errors': []}
            local.retain_result(project, output, record)
            self.assertEqual(record['status'], 'failed')
            self.assertEqual(record['collection_errors'][0]['stage'], 'source capture')
            self.assertFalse((project / 'target').is_symlink())
            self.assertEqual(keep.read_text(), 'preserve unrelated data')

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
            prompt = local.prompt('access-proof', setup, {name: {'path': '/' + name} for name in ('python', 'cargo', 'lake')}, 'M1')
            self.assertIn(local.tasks.TASKS['access-proof']['request'], prompt)
            self.assertNotIn('rfl', prompt)
            self.assertIn('change.patch', prompt)
            self.assertIn('Do not start a nested guard', prompt)
            self.assertNotIn('.fr/checks.json', prompt)
            self.assertNotIn('.fr/check-escape', prompt)

    def test_each_m2_requirement_stays_visible_and_unchanged(self):
        from agent_eval import project_edits_tasks as cases
        checkers = {name: {'path': '/' + name} for name in ('python', 'cargo', 'rustc', 'node', 'typescript')}
        for task in cases.TASKS:
            for setup in ('files', 'fr'):
                with self.subTest(task=task, setup=setup):
                    prompt = local.prompt(task, setup, checkers, 'M2')
                    listed = prompt.split('\n\n', 1)[0].splitlines()
                    self.assertEqual(listed[0], 'Complete every requirement:')
                    self.assertTrue(all(line.startswith('- ') for line in listed[1:]))
                    self.assertEqual(' '.join(line[2:] for line in listed[1:]), cases.TASKS[task]['request'])
                    self.assertIn('150-second deadline', prompt)
                    self.assertIn('Preserve provided .fr/checks.json', prompt)
                    self.assertEqual('.fr/check-escape' in prompt, task == 'regex-escape-len')

    def test_execution_clock_keeps_preparation_inside_the_original_deadline(self):
        prompt = local.execution_prompt('Frozen request.', started_at=1000.125,
                                        wall_seconds=150, finish_seconds=10)
        self.assertIn('start 1000.125; finish tools by 1140.125', prompt)
        self.assertIn('response and session accounting by 1150.125', prompt)
        self.assertTrue(prompt.endswith('Frozen request.'))
        for finishing in (0, 150, 180):
            with self.assertRaisesRegex(ValueError, 'within the attempt deadline'):
                local.execution_prompt('Frozen request.', started_at=1000.125,
                                       wall_seconds=150, finish_seconds=finishing)

    def test_public_behavior_check_uses_the_actual_attempt_deadline(self):
        from agent_eval import project_edits_public as checker
        checkers = {name: {'path': '/' + name} for name in ('python', 'cargo', 'rustc', 'node', 'typescript')}
        frozen = local.prompt('regex-escape-len', 'fr', checkers, 'M2')
        self.assertIn(checker.CHECKER, frozen)
        self.assertIn(local.TOOL_DEADLINE_MARKER, frozen)
        prompt = local.execution_prompt(frozen, started_at=1000.125, wall_seconds=150, finish_seconds=10)
        self.assertNotIn(local.TOOL_DEADLINE_MARKER, prompt)
        command = prompt.split('Available project checking commands:\n')[1].splitlines()[0]
        argv = shlex.split(command)
        self.assertEqual(argv[argv.index('--deadline-unix') + 1], '1140.125')
        self.assertEqual(local.PUBLIC_CHECK_ENVIRONMENT['CARGO_PROFILE_DEV_OPT_LEVEL'], '0')

    def test_checking_inputs_cannot_be_changed_deleted_or_replaced_with_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / '.fr/checks.json'
            config.parent.mkdir()
            config.write_text('{"checks": []}')
            expected = {'.fr/checks.json': local.hash_file(config)}
            self.assertEqual(local.checking_input_changes(root, expected), [])
            config.write_text('{}')
            self.assertEqual(local.checking_input_changes(root, expected), ['.fr/checks.json'])
            config.unlink()
            self.assertEqual(local.checking_input_changes(root, expected), ['.fr/checks.json'])
            other = root / 'same-bytes'
            other.write_text('{"checks": []}')
            config.symlink_to(other)
            self.assertEqual(local.checking_input_changes(root, expected), ['.fr/checks.json'])
            config.unlink()
            config.write_text('{"checks": []}')
            config.parent.rename(root / 'old-settings')
            config.parent.symlink_to(root / 'old-settings', target_is_directory=True)
            self.assertEqual(local.checking_input_changes(root, expected), ['.fr/checks.json'])

    def test_v2_export_pins_model_variant_and_agrees_with_stream_usage(self):
        profile = {'model': {'providerID': 'kimi-code-plan-global', 'id': 'k3', 'variant': 'high'}}
        saved = {'info': {'id': 'ses_example', 'agent': 'build', 'model': profile['model'], 'outcome': 'succeeded'},
                 'messages': [{'type': 'agent-switched', 'agent': 'build'},
                     {'id': 'msg_example', 'type': 'assistant', 'agent': 'build', 'model': profile['model'], 'finish': 'stop',
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

    def test_interrupted_v2_session_retains_an_absent_final_outcome_and_unknown_usage(self):
        profile = {'model': {'providerID': 'zai-coding-plan', 'id': 'glm-5.3-flash', 'variant': 'high'}}
        saved = {'info': {'id': 'ses_interrupted', 'agent': 'build', 'model': profile['model']},
                 'messages': [{'id': 'msg_partial', 'type': 'assistant', 'agent': 'build', 'model': profile['model'],
                               'finish': 'error', 'tokens': None, 'cost': None}]}
        result = verify_export(saved, profile, 'ses_interrupted', [])
        self.assertIsNone(result['outcome'])
        self.assertEqual(result['unfinished_assistant_responses'], 1)
        self.assertEqual(result['stream_finished_responses'], 0)
        self.assertTrue(result['model_observed'])
        self.assertIsNone(saved['messages'][0]['tokens'])
        del saved['messages'][0]['finish']
        self.assertEqual(verify_export(saved, profile, 'ses_interrupted', [])['unfinished_assistant_responses'], 1)


if __name__ == '__main__':
    unittest.main()
