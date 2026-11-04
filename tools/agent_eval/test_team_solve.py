"""Scripted team controls: no credentials, model requests or compiled fr needed."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import team_solve as team, team_solve_native as native
from agent_eval.selected_plan import ROOT, encoded


class TeamSolveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.run = self.root / 'run';self.run.mkdir()
        fixture = ROOT / 'tests/agent-eval/team-solve'
        shutil.copytree(fixture / 'project', self.run / 'seed')
        shutil.copyfile(fixture / 'task.md', self.run / 'task.md')
        shutil.copytree(ROOT / 'skills/fr', self.run / 'skill')
        profiles = team.load(ROOT / 'tests/agent-eval/proof-writing/profiles.json')
        self.settings = {
            'workflow': 'team',
            'project': str(self.root / 'original'), 'task': str(self.run / 'task.md'),
            'output': str(self.run), 'profiles': str(ROOT / 'tests/agent-eval/proof-writing/profiles.json'),
            'binaries': {name: sys.executable for name in ('fr', 'codex', 'opencode2')},
            'fr_build': str(self.root),
            'roles': dict(zip(team.ROLES, ('codex-luna-fast', 'opencode2-glm-flash', 'opencode2-kimi-k3',
                                         'codex-luna-fast', 'opencode2-kimi-k3', 'codex-luna-fast'))),
            'editable_files': ['counter.py'], 'editable_regions': [], 'protected_files': ['check.py'],
            'reference_directories': [], 'require_change': True, 'check_environment': {'PYTHONDONTWRITEBYTECODE': '1'},
            'limits': {'prepare_seconds': 60, 'role_seconds': 150, 'check_seconds': 120, 'run_seconds': 1200, 'parallel_roles': 1},
            'checks': [{'name': 'page examples', 'argv': [sys.executable, '-B', 'check.py'], 'cwd': '.',
                        'timeout_seconds': 20, 'covers': ['all eight supplied page counts'], 'verdict': 'exit-code'}],
        }
        self.plan = {'settings': self.settings, 'profiles': profiles, 'source': team.sources(self.run / 'seed'),
                     'regions': {}, 'binaries': {}, 'companions': {}, 'implementation': {},
                     'checker_hashes': {sys.executable: team.local.hash_file(Path(sys.executable))},
                     'skill': team.tree_hashes(self.run / 'skill'), 'task_sha256': team.local.hash_file(self.run / 'task.md')}
        self.freeze()
        team.copy_source(self.run / 'seed', self.run / 'candidate');team.initialize(self.run / 'candidate')
        for directory in ('records', 'artifacts', 'work'):
            (self.run / directory).mkdir()
        self.calls = []

    def freeze(self):
        self.plan.pop('sha256', None)
        self.plan['sha256'] = hashlib.sha256(encoded(self.plan)).hexdigest()
        team.common.save(self.run / 'plan.json', self.plan)

    def agent(self, plan, project, record, role, prompt, seconds):
        self.calls.append((role, project))
        self.assertIn('Source and test paths in the task are relative to this root.', prompt)
        self.assertIn('not copies of source or test files', prompt)
        inbox = project / team.DATA / 'in'
        if role in {'ideate-a', 'ideate-b'}:
            self.assertEqual({p.name for p in inbox.iterdir()}, {'task.md', 'plan.md'})
            self.assertEqual((project / 'counter.py').read_bytes(), (self.run / 'seed/counter.py').read_bytes())
        if role in {'review', 'repair'}:
            self.assertTrue((inbox / 'check-logs/0.stdout').is_file())
            self.assertIn('passed', json.loads((inbox / 'initial-check.json').read_text()))
        if role in {'solve', 'repair'}:
            path = project / 'counter.py'
            path.write_text(path.read_text().replace('return items // size', 'return (items + size - 1) // size'))
        (project / team.DATA / 'out' / team.OUTPUTS[role]).write_text('Scripted handoff; no model call.\n')
        return {'status': 'completed', 'process': {'exit_code': 0, 'stop_reason': None}, 'usage': {'actual_cost': None}}

    def drive(self, agent):
        def worker(run, operation, expected, name, seconds):
            plan = team.verified_plan(run, expected)
            if operation == '_stage':
                team.stage(plan, run, name, seconds, agent)
            else:
                team.check(plan, run, name, seconds)
        return team.execute(self.run, self.plan, team.elapsed_time(), worker)

    def test_success_independent_proposals_fresh_context_and_no_repair(self):
        result = self.drive(self.agent)
        self.assertTrue(result['passed'])
        self.assertEqual([r for r, p in self.calls], list(team.ROLES[:-1]))
        self.assertEqual(len({p for r, p in self.calls}), 5)
        self.assertEqual(result['roles']['repair']['status'], 'skipped')
        self.assertIn(b'+    return (items + size - 1) // size', (self.run / 'change.patch').read_bytes())
        self.assertEqual(team.sources(self.run / 'seed'), self.plan['source'])
        self.assertTrue((self.run / 'records/solve/candidate.tar.gz').is_file())
        public = json.loads((self.run / 'summary.json').read_text())
        self.assertIsNone(public['actual_cost'])
        self.assertNotIn(str(self.root), json.dumps(public))

    def test_one_repair_gets_real_failure_then_final_checks_pass(self):
        def agent(*args):
            result = self.agent(*args)
            if args[3] == 'solve':
                shutil.copyfile(self.run / 'seed/counter.py', args[1] / 'counter.py')
            if args[3] == 'repair':
                report = json.loads((args[1] / team.DATA / 'in/initial-check.json').read_text())
                self.assertFalse(report['passed'])
                self.assertIn('AssertionError', (args[1] / team.DATA / 'in/check-logs/0.stderr').read_text())
            return result
        result = self.drive(agent)
        self.assertTrue(result['passed'])
        self.assertEqual([r for r, p in self.calls], list(team.ROLES))

    def test_all_luna_team_control_changes_only_the_explicit_assignments(self):
        self.settings['roles'] = {role: 'codex-luna-fast' for role in team.ROLES}
        path = self.root / 'settings.json';team.common.save(path, self.settings);team.settings(path)
        self.freeze()
        result = self.drive(self.agent)
        self.assertTrue(result['passed'])
        self.assertEqual(result['workflow'], 'team')
        self.assertEqual([r for r, p in self.calls], list(team.ROLES[:-1]))
        for role, project in self.calls:
            self.assertEqual(result['roles'][role]['profile'], 'codex-luna-fast')

    def test_single_control_has_one_call_no_handoffs_and_the_same_final_checker(self):
        self.settings.update(workflow='single', roles={'solve': 'codex-luna-fast'})
        path = self.root / 'settings.json';team.common.save(path, self.settings);team.settings(path)
        self.freeze()
        def agent(*args):
            inbox = args[1] / team.DATA / 'in'
            self.assertEqual({p.name for p in inbox.iterdir()}, {'task.md'})
            self.assertIn('one direct agent session', args[4])
            self.assertNotIn('Read both proposals', args[4])
            return self.agent(*args)
        result = self.drive(agent)
        self.assertTrue(result['passed'])
        self.assertEqual([r for r, p in self.calls], ['solve'])
        self.assertEqual(set(result['roles']), {'solve', 'final-check'})
        self.assertEqual(result['workflow'], 'single')
        self.assertIsNone(team.load(self.run / 'summary.json')['provider_request_count'])
        self.assertEqual(result['roles']['final-check']['checks'][0]['name'], self.settings['checks'][0]['name'])

    def test_failed_single_control_cannot_use_a_hidden_repair(self):
        self.settings.update(workflow='single', roles={'solve': 'codex-luna-fast'})
        self.freeze()
        def agent(*args):
            result = self.agent(*args)
            shutil.copyfile(self.run / 'seed/counter.py', args[1] / 'counter.py')
            return result
        result = self.drive(agent)
        self.assertFalse(result['passed'])
        self.assertEqual([r for r, p in self.calls], ['solve'])
        self.assertNotIn('repair', result['roles'])

    def test_all_roles_share_the_one_declared_complete_budget(self):
        self.settings['limits']['run_seconds'] = 1
        self.freeze()
        now = [100.0]
        def worker(run, operation, expected, name, seconds):
            self.assertEqual(name, 'plan')
            self.assertLessEqual(seconds, 1)
            team.stage(self.plan, run, name, 30, self.agent)
            now[0] += 2
        with patch.object(team, 'elapsed_time', side_effect=lambda: now[0]):
            with self.assertRaisesRegex(ValueError, 'complete run deadline'):
                team.execute(self.run, self.plan, 100.0, worker)
        self.assertEqual([r for r, p in self.calls], ['plan'])
        self.assertFalse(team.load(self.run / 'summary.json')['passed'])

    def test_passing_patch_cannot_override_an_expired_complete_run_budget(self):
        self.settings.update(workflow='single', roles={'solve': 'codex-luna-fast'})
        self.settings['limits']['run_seconds'] = 1
        self.freeze()
        now = [100.0]
        def worker(run, operation, expected, name, seconds):
            if operation == '_stage':
                team.stage(self.plan, run, name, 30, self.agent)
            else:
                team.check(self.plan, run, name, 30)
                now[0] += 2
        with patch.object(team, 'elapsed_time', side_effect=lambda: now[0]):
            result = team.execute(self.run, self.plan, 100.0, worker)
        self.assertTrue(result['roles']['final-check']['passed'])
        self.assertFalse(result['passed'])
        self.assertEqual(result['status'], 'failed')

    def test_failed_final_check_cannot_be_overruled_by_handoff(self):
        def agent(*args):
            result = self.agent(*args)
            if args[3] in {'solve', 'repair'}:
                shutil.copyfile(self.run / 'seed/counter.py', args[1] / 'counter.py')
            return result
        self.assertFalse(self.drive(agent)['passed'])
        self.assertEqual(sum(r == 'repair' for r, p in self.calls), 1)

    def test_reviewer_cannot_overwrite_passing_candidate(self):
        def agent(*args):
            result = self.agent(*args)
            if args[3] == 'review':
                (args[1] / 'counter.py').write_text('broken\n')
            return result
        with self.assertRaisesRegex(ValueError, 'planning or review'):
            self.drive(agent)
        self.assertIn('items + size - 1', (self.run / 'candidate/counter.py').read_text())
        self.assertTrue((self.run / 'records/review/candidate.tar.gz').exists())
        self.assertEqual(team.load(self.run / 'summary.json')['roles']['review']['status'], 'failed')

    def test_protected_checker_and_settings_changes_are_failures(self):
        for filename in ('check.py', '.fr/checks.json'):
            with self.subTest(filename=filename):
                run = self.root / ('tamper-' + Path(filename).name)
                shutil.copytree(self.run, run)
                def agent(*args):
                    result = self.agent(*args)
                    (args[1] / filename).write_text('{}')
                    return result
                with self.assertRaises(ValueError):
                    team.stage(self.plan, run, 'plan', 30, agent)
                self.assertEqual(team.load(run / 'records/plan/result.json')['status'], 'failed')

    def test_lean_claim_is_fixed_outside_tactics(self):
        candidate = self.run / 'candidate'
        lean = candidate / 'proof.lean'
        lean.write_text('theorem keep : True := by\n-- begin\n  sorry\n-- end\n')
        rule = {'path': 'proof.lean', 'begin': '-- begin', 'end': '-- end'}
        self.plan['source'] = team.sources(candidate)
        self.settings['editable_files'] = []
        self.settings['editable_regions'] = [rule]
        self.plan['regions'] = {'proof.lean': team.fixed_region_hash(lean.read_text(), rule)}
        lean.write_text(lean.read_text().replace('sorry', 'trivial'))
        team.boundary(self.plan, candidate)
        lean.write_text(lean.read_text().replace('True', 'False'))
        with self.assertRaisesRegex(ValueError, 'claim'):
            team.boundary(self.plan, candidate)

    def test_zero_exit_with_passed_false_fails_json_check(self):
        self.settings['require_change'] = False
        self.settings['checks'][0].update(argv=[sys.executable, '-c', 'print(\'{"passed": false}\')'], verdict='json-passed')
        result = team.check(self.plan, self.run, 'initial-check', 30)
        self.assertFalse(result['passed'])
        self.assertEqual(result['checks'][0]['process']['exit_code'], 0)

    def test_failed_native_call_kept_and_never_retried(self):
        def agent(*args):
            report = self.agent(*args)
            report.update(status='failed', process={'exit_code': -9, 'stop_reason': 'rss_bytes'})
            return report
        with self.assertRaisesRegex(ValueError, 'do not retry'):
            self.drive(agent)
        self.assertEqual(len(self.calls), 1)
        self.assertTrue((self.run / 'records/plan/candidate.tar.gz').exists())
        with self.assertRaises(FileExistsError):
            self.drive(agent)
        self.assertEqual(len(self.calls), 1)

    def test_checker_resource_stop_is_retained_and_stops_team(self):
        with patch.object(team, 'bounded_run', return_value={'stop_reason': 'rss_bytes', 'exit_code': -9}):
            with self.assertRaisesRegex(ValueError, 'resource'):
                self.drive(self.agent)
        self.assertEqual([r for r, p in self.calls], ['plan', 'ideate-a', 'ideate-b', 'solve'])
        self.assertEqual(team.load(self.run / 'records/initial-check/logs/0.process.json')['stop_reason'], 'rss_bytes')

    def test_explicit_settings_and_caps(self):
        path = self.root / 'settings.json'
        team.common.save(path, self.settings)
        team.settings(path)
        for mutate in (lambda v: v.pop('protected_files'), lambda v: v.pop('workflow'),
                       lambda v: v.update(workflow='invented'),
                       lambda v: v.update(workflow='single'), lambda v: v['limits'].update(role_seconds=151),
                       lambda v: v['limits'].update(parallel_roles=2), lambda v: v['roles'].update(solve='some-stronger-model'),
                       lambda v: v.update(protected_files=['counter.py'])):
            value = copy.deepcopy(self.settings);mutate(value);team.common.save(path, value)
            with self.assertRaises((ValueError, KeyError)):
                team.settings(path)

    def test_skill_and_task_changes_refuse_continuation(self):
        (self.run / 'skill/SKILL.md').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'skill changed'):
            team.verified_plan(self.run, self.plan['sha256'])

    def test_prepare_captures_native_versions_and_source_before_calls(self):
        original = Path(self.settings['project'])
        team.copy_source(self.run / 'seed', original);team.initialize(original)
        install = self.root / 'installed';install.mkdir()
        for name, reply in {'codex': 'codex-cli 0.164.0', 'opencode2': 'opencode v2.2.5',
                            'fr': 'fr fixture', 'codex-code-mode-host': 'companion', 'opencode': 'companion'}.items():
            path = install / name;path.write_text('#!/bin/sh\nprintf "' + reply + '\\n"\n');path.chmod(0o755)
        value = copy.deepcopy(self.settings)
        value['binaries'] = {k: str(install / k) for k in ('fr', 'codex', 'opencode2')}
        prepared = self.root / 'prepared';prepared.mkdir();value['output'] = str(prepared)
        with patch.object(team.local, 'identity', return_value={}), patch.object(team.local, 'verified_fr_build', return_value={'fixture': True}):
            team.prepare(value, self.plan['profiles'], prepared)
        plan = team.load(prepared / 'plan.json')
        team.verified_plan(prepared, plan['sha256'])
        self.assertEqual(plan['versions']['opencode2'], 'opencode v2.2.5')
        self.assertEqual(plan['source'], self.plan['source'])
        self.assertEqual(set(plan['companions']), {'codex-code-mode-host', 'opencode'})
        self.assertEqual(team.sources(original), self.plan['source'])

    def test_native_uses_exact_models_and_enforces_real_completion(self):
        for profile in self.plan['profiles']:
            with self.subTest(profile=profile):
                role = profile
                project = self.root / profile;project.mkdir();(project / '.fr').mkdir()
                record = self.root / (profile + '-record');record.mkdir()
                plan = copy.deepcopy(self.plan)
                plan['settings']['roles'][role] = profile
                plan['binaries'] = {k: {'path': sys.executable} for k in ('fr', 'codex', 'opencode2')}
                is_codex = plan['profiles'][profile]['client'] == 'codex'
                def run(argv, stdin, stdout, stderr, workspace, **limits):
                    self.assertEqual(limits['rss_bytes'], 768 * 1024**2)
                    self.assertLessEqual(limits['wall_seconds'], 30)
                    self.assertEqual(limits['cpu_limit_seconds'], 75)
                    if is_codex:
                        self.assertIn('gpt-6-luna', argv)
                        self.assertIn('service_tier="fast"', argv)
                        stdout.write(b'{"type":"turn.completed","usage":{"input_tokens":1}}\n');stdout.flush()
                    else:
                        selection = plan['profiles'][profile]['model']
                        self.assertIn(selection['providerID'] + '/' + selection['id'] + '#low', argv)
                        session = argv[argv.index('--session') + 1]
                        stdout.write(json.dumps({'type': 'text', 'sessionID': session}).encode() + b'\n');stdout.flush()
                    return {'exit_code': 0, 'stop_reason': None}
                def accounting(report, *args):
                    report['session_accounting'] = {'status': 'completed'}
                with patch.object(native, 'run', side_effect=run), patch.object(native.local, 'record_opencode_accounting', side_effect=accounting):
                    result = native.agent(plan, project, record, role, 'Do the scripted task.', 30)
                self.assertEqual(result['status'], 'completed')
                self.assertIsNone(result['usage']['actual_cost'])

    def test_retained_live_patch_passes_the_unchanged_eight_case_checker(self):
        retained = ROOT / 'tests/agent-eval/team-solve'
        summary = team.load(retained / 'pilot-summary.json')
        patch_bytes = (retained / 'pilot.patch').read_bytes()
        self.assertEqual(hashlib.sha256(patch_bytes).hexdigest(), summary['patch_sha256'])
        self.assertEqual(team.sources(self.run / 'seed'), summary['pilot']['source'])
        candidate = self.run / 'candidate'
        original = subprocess.run([sys.executable, '-B', 'check.py'], cwd=candidate, capture_output=True)
        self.assertNotEqual(original.returncode, 0, 'the unchanged fixture must catch the original mistake')
        git = team.common.comparison_grade.BASE.git
        git(candidate, 'apply', '--check', '-', data=patch_bytes)
        git(candidate, 'apply', '-', data=patch_bytes)
        result = subprocess.run([sys.executable, '-B', 'check.py'], cwd=candidate, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        team.boundary(self.plan, candidate)
        self.assertTrue(summary['passed'])
        self.assertEqual(summary['roles']['repair']['status'], 'skipped')
        self.assertEqual(summary['pilot']['actual_fr_invocations'], 0)
        review = team.load(retained / 'pilot-time-review.json')
        self.assertEqual(review['source_summary_sha256'], team.local.hash_file(retained / 'pilot-summary.json'))
        measured = review['observed']
        self.assertAlmostEqual(sum(measured[k] for k in ('planning_and_coding_seconds', 'proposals_and_review_seconds',
            'declared_check_seconds', 'remaining_preparation_and_coordination_seconds')), summary['elapsed_seconds'])
        self.assertFalse(measured['within_original_150_second_solo_limit'])
        self.assertEqual(review['accounting_clarification']['native_cli_sessions'], sum('process' in row for row in summary['roles'].values()))
        self.assertIsNone(review['accounting_clarification']['provider_request_count'])


if __name__ == '__main__':
    unittest.main()
