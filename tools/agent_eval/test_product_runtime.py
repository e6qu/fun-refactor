"""Refuse incomplete native admission without launching clients or spending money."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from . import algorithm_admission as native, product_plan as plan, product_runtime as runtime
from . import selected_guarded, selected_local as local
from .selected_plan import ROOT


class ProductRuntimeTests(unittest.TestCase):
    def test_same_public_checks_preserved_for_both_setups_and_new_files_delivered(self):
        checkers = {'python': {'path': '/python'}, 'lake': {'path': '/lean/bin/lake'},
                    'lean': {'source_root': '/lean/src/lean'}}
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            protected = runtime.install(project, 'slugify-bytes', checkers)
            self.assertEqual(set(protected), {'.fr/checks.json', '.fr/product-check.py'})
            self.assertEqual(local.checking_input_changes(project, protected), [])
            for setup in ('files', 'fr'):
                prompt = local.prompt('slugify-bytes', setup, checkers, 'P5-initial')
                self.assertIn('/python -B .fr/product-check.py slugify-bytes', prompt)
                self.assertIn('git add -N .', prompt)
                self.assertIn('150-second deadline', prompt)
            (project / '.fr/product-check.py').write_text('pass\n')
            self.assertEqual(local.checking_input_changes(project, protected), ['.fr/product-check.py'])

    def test_script_uses_public_checks_and_delivers_new_files_without_live_calls(self):
        checkers = {'python': {'path': '/python'}, 'lake': {'path': '/lean/bin/lake'}}
        for task in plan.TASKS['initial']:
            for setup in ('files', 'fr'):
                text = runtime.admission_script(task, setup, checkers, '/pinned:/usr/bin:/bin')
                self.assertIn('git apply .fr/admission-reference.patch', text)
                self.assertIn('git add -N .', text)
                self.assertIn('git diff --binary HEAD > change.patch', text)
                self.assertEqual('fr --json' in text, setup == 'fr')
                self.assertIn('/lean/bin/lake -d specs build --wfail' if task.endswith('-proof') else
                              '/python -B .fr/product-check.py ' + task, text)

    def test_all_24_admissions_and_their_resource_reserve_are_required(self):
        schedule = plan.prepare(ROOT, 'P5-initial')
        binaries = {name: {'sha256': name + '-hash'} for name in ('fr', 'codex', 'opencode2')}
        checkers = {name: {'sha256': name + '-hash'} for name in ('python', 'lean', 'lake', 'leanc')}
        seeds = {task: {'receipt': {'archive_sha256': task + '-hash'}} for task in schedule['requirements']['tasks']}
        with tempfile.TemporaryDirectory() as temporary:
            guard = Path(temporary) / 'guard.py'
            guard.write_text('guard')
            records = {}
            for profile, settings in schedule['profiles'].items():
                for task in seeds:
                    for setup in ('files', 'fr'):
                        key = profile + '/' + task + '/' + setup
                        records[key] = {'passed': True, 'complete_workflow': True, 'session_accounting': True,
                            'independent_patch_replay': True, 'study': 'P5-initial', 'profile': profile,
                            'task': task, 'setup': setup, 'live_model_calls': 0, 'upstream_forwarding': False,
                            'scripted_requests': 2, 'scripted_errors': 0, 'implementation': {'fixed': 'input'},
                            'qualified_run': '123', 'archive_sha256': seeds[task]['receipt']['archive_sha256'],
                            'fr_sha256': binaries['fr']['sha256'], 'client_sha256': binaries[settings['client']]['sha256'],
                            'guard_sha256': runtime.common.digest(guard), 'client_context_reserve_bytes': runtime.RESERVE_BYTES,
                            'checker_sha256': {key: checkers[key]['sha256'] for key in ('lean', 'lake', 'leanc')},
                            'python_runtime': {'executable_sha256': checkers['python']['sha256'],
                                'requirements_sha256': runtime.common.digest(native.NATIVE_REQUIREMENTS),
                                'packages': native.server_packages()},
                            'resources': {'limits': {'wall_seconds': 60, 'rss_bytes': 805306368, 'cpu_seconds': 25,
                                'disk_bytes': 67108864, 'transcript_bytes': 131072}, 'exit_code': 0, 'stop_reason': None,
                                'monitor_error': None, 'launch_error': None, 'elapsed_seconds': 10,
                                'sampled_aggregate_rss_bytes': 500000000, 'sampled_cpu_seconds': 3,
                                'sampled_disk_growth_bytes': 1000, 'transcript_bytes': 1000}}
            with patch.object(selected_guarded, 'GUARD', guard), \
                    patch.object(local, 'implementation_bindings', return_value={'fixed': 'input'}):
                def validate(rows):
                    return runtime.validate_receipts(rows, binaries, seeds, '123', 'P5-initial', checkers)
                self.assertEqual(validate(records)['admitted_combinations'], 24)
                key = next(iter(records))
                missing = copy.deepcopy(records)
                missing.pop(key)
                with self.assertRaisesRegex(ValueError, 'all 24'):
                    validate(missing)
                for field, value in (('session_accounting', False), ('task', 'another-task'),
                                     ('implementation', {}), ('archive_sha256', 'stale'),
                                     ('client_sha256', 'stale'), ('client_context_reserve_bytes', 0)):
                    changed = copy.deepcopy(records)
                    changed[key][field] = value
                    with self.subTest(field=field), self.assertRaises(ValueError):
                        validate(changed)
                for value in (805306368, True, -1, float('nan')):
                    changed = copy.deepcopy(records)
                    changed[key]['resources']['sampled_aggregate_rss_bytes'] = value
                    with self.subTest(rss=value), self.assertRaises(ValueError):
                        validate(changed)


if __name__ == '__main__':
    unittest.main()
