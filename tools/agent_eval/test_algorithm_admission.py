"""Initial-goal spot checks cannot stand in for complete native admission."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import algorithm_admission as admission


class AlgorithmAdmissionChecks(unittest.TestCase):
    def test_complete_script_checks_rejection_both_proofs_package_and_delivery(self):
        script = admission.script(Path('/pinned-lean'), '/pinned-tools:/usr/bin:/bin', 'M3-algorithm-facts', 'workflow')
        self.assertIn('export PATH=/pinned-tools:/usr/bin:/bin', script)
        self.assertIn('Possibly looping simp theorem', script)
        self.assertIn('/pinned-lean/bin/lake -d specs build --wfail', script)
        self.assertIn('git diff --binary HEAD > change.patch', script)
        self.assertEqual(script.count(' spec proof-check '), 3)
        self.assertEqual(script.count(' spec prove '), 2)
        for name in admission.algorithms.OBLIGATIONS:
            self.assertIn(str(admission.ROOT / admission.algorithms.DIRECTORY / 'controls' /
                              (name.replace('_', '-') + '.lean')), script)

    def test_admission_rejects_spot_checks_stale_inputs_and_inadequate_margin(self):
        profiles = json.loads((admission.ROOT / 'tests/agent-eval/proof-writing/profiles.json').read_text())
        binaries = {'fr': {'sha256': 'f' * 64}, 'opencode2': {'sha256': 'a' * 64}, 'codex': {'sha256': 'b' * 64}}
        checkers = {name: {'sha256': name + '-hash'} for name in ('python', 'lean', 'lake', 'leanc')}
        with tempfile.TemporaryDirectory() as temporary:
            guard = Path(temporary) / 'guard.py'
            guard.write_text('guard')
            rows = {name: {'passed': True, 'profile': name, 'study': 'M3-algorithm-facts', 'setup': 'workflow',
                'checker_sha256': {key: checkers[key]['sha256'] for key in ('lean', 'lake', 'leanc')},
                'python_runtime': {'executable_sha256': checkers['python']['sha256'],
                    'requirements_sha256': admission.common.digest(admission.NATIVE_REQUIREMENTS),
                    'packages': admission.server_packages()},
                'live_model_calls': 0, 'upstream_forwarding': False,
                'scripted_requests': 2, 'scripted_errors': 0, 'complete_workflow': True, 'recursive_loop_rejected': True,
                'session_accounting': True, 'fr_sha256': 'f' * 64, 'client_sha256': binaries[profile['client']]['sha256'],
                'archive_sha256': 'c' * 64, 'guard_sha256': admission.common.digest(guard), 'qualified_run': '12',
                'implementation': {'tool': 'hash'}, 'client_context_reserve_bytes': admission.RESERVE_BYTES,
                'resources': {'limits': {'wall_seconds': 60, 'rss_bytes': 805306368, 'cpu_seconds': 25,
                    'disk_bytes': 67108864, 'transcript_bytes': 131072}, 'exit_code': 0, 'stop_reason': None,
                    'monitor_error': None, 'launch_error': None, 'elapsed_seconds': 10,
                    'sampled_aggregate_rss_bytes': 500000000, 'sampled_cpu_seconds': 3,
                    'sampled_disk_growth_bytes': 1000, 'transcript_bytes': 1000}} for name, profile in profiles.items()}
            with patch.object(admission, 'GUARD', guard), \
                    patch.object(admission.local, 'implementation_bindings', return_value={'tool': 'hash'}):
                result = admission.validate_receipts(rows, binaries, 'c' * 64, '12', 'M3-algorithm-facts', checkers)
                self.assertEqual(set(result['profiles']), set(profiles))
                self.assertNotIn('implementation', result['profiles']['codex-luna-fast'])
                for field, value in [('complete_workflow', False), ('recursive_loop_rejected', False),
                                     ('session_accounting', False), ('archive_sha256', 'changed'),
                                     ('client_context_reserve_bytes', 0), ('implementation', {}), ('python_runtime', {})]:
                    changed = copy.deepcopy(rows)
                    changed['opencode2-glm-flash'][field] = value
                    with self.subTest(field=field), self.assertRaises(ValueError):
                        admission.validate_receipts(changed, binaries, 'c' * 64, '12', 'M3-algorithm-facts', checkers)
                for value in (805306368, 10**500, float('nan'), -1, True):
                    changed = copy.deepcopy(rows)
                    changed['codex-luna-fast']['resources']['sampled_aggregate_rss_bytes'] = value
                    with self.subTest(rss=value), self.assertRaises(ValueError):
                        admission.validate_receipts(changed, binaries, 'c' * 64, '12', 'M3-algorithm-facts', checkers)
                delivery_rows = {}
                for name, original in rows.items():
                    for setup in admission.delivery.SETUPS:
                        delivery_rows[name + '/' + setup] = {**copy.deepcopy(original), 'study': admission.DELIVERY_STUDY,
                            'setup': setup, 'supplied_proofs': True, 'saved_edit_export': True, 'independent_patch_replay': True}
                result = admission.validate_receipts(delivery_rows, binaries, 'c' * 64, '12', admission.DELIVERY_STUDY, checkers)
                self.assertEqual(len(result['profiles']), 6)
                for field, value in (('study', 'M3-algorithm-facts'), ('setup', 'workflow'),
                                     ('saved_edit_export', False), ('independent_patch_replay', False),
                                     ('checker_sha256', {}), ('supplied_proofs', False)):
                    changed = copy.deepcopy(delivery_rows)
                    changed['opencode2-glm-flash/finish-card'][field] = value
                    with self.subTest(delivery_field=field), self.assertRaises(ValueError):
                        admission.validate_receipts(changed, binaries, 'c' * 64, '12', admission.DELIVERY_STUDY, checkers)
                del delivery_rows['opencode2-kimi-k3/shared-skill']
                with self.assertRaisesRegex(ValueError, 'every profile'):
                    admission.validate_receipts(delivery_rows, binaries, 'c' * 64, '12', admission.DELIVERY_STUDY, checkers)
                repair_rows = {name + '/' + setup: {**row, 'study': admission.REPAIR_STUDY, 'setup': setup,
                    'interactive_goal': True, 'argument_type_rejection': True, 'independent_patch_replay': True}
                    for name, row in rows.items() if name != 'codex-luna-fast'
                    for setup in ('shared-skill', 'argument-repair')}
                admitted = admission.validate_receipts(repair_rows, binaries, 'c' * 64, '12', admission.REPAIR_STUDY, checkers)
                self.assertEqual(len(admitted['profiles']), 4)
                for field, value in (('interactive_goal', False), ('argument_type_rejection', False), ('session_accounting', False),
                                     ('independent_patch_replay', False), ('client_sha256', 'stale')):
                    changed = copy.deepcopy(repair_rows)
                    changed['opencode2-glm-flash/argument-repair'][field] = value
                    with self.subTest(repair_field=field), self.assertRaises(ValueError):
                        admission.validate_receipts(changed, binaries, 'c' * 64, '12', admission.REPAIR_STUDY, checkers)

    def test_delivery_admission_uses_supplied_drafts_and_both_saved_edits(self):
        for setup in admission.delivery.SETUPS:
            script = admission.script(Path('/pinned-lean'), '/pinned-tools:/usr/bin:/bin', admission.DELIVERY_STUDY, setup)
            self.assertIn('.fr/euclid_decreases.tactics', script)
            self.assertIn('.fr/euclid_correct.tactics', script)
            self.assertEqual(script.count(' spec proof-check '), 2)
            self.assertEqual(script.count(' spec prove '), 2)
            self.assertIn('"history","patch"', script)
            self.assertNotIn('git diff', script)
            self.assertEqual('.fr/finish-card.txt' in script, setup == 'finish-card')

    def test_scripted_server_dependency_is_explicit_and_checked_before_admission(self):
        self.assertEqual(admission.server_packages(), {'websockets': '15.0.1'})
        with patch.object(admission.metadata, 'version', side_effect=admission.metadata.PackageNotFoundError('websockets')):
            with self.assertRaisesRegex(ValueError, 'Selected Python lacks'):
                admission.native_python(admission.platform.python_version())
        with patch.object(admission.metadata, 'version', return_value='16.0'):
            with self.assertRaisesRegex(ValueError, 'hash-locked requirement'):
                admission.native_python(admission.platform.python_version())
        with patch.object(admission.metadata, 'version', return_value='15.0.1'):
            self.assertEqual(admission.native_python(admission.platform.python_version())['packages'], admission.server_packages())

    def test_python_pin_is_checked_before_package_inventory_and_native_work(self):
        with patch.object(admission.platform, 'python_version', return_value='3.14.11'), \
                patch.object(admission.metadata, 'version') as inventory:
            with self.assertRaisesRegex(ValueError, 'Python differs from its explicit pin'):
                admission.native_python('3.14.10')
            inventory.assert_not_called()


if __name__ == '__main__':
    unittest.main()
