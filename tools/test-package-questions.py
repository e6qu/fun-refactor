"""Protect the unread helper selection and retain an empty selection as a failure."""
import importlib.util
import copy
import json
import subprocess
import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_questions', ROOT / 'tools/package-questions.py')
questions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(questions)


class IndependentRuntime(unittest.TestCase):
    def test_project_imports_do_not_reuse_the_collectors_sdk_or_tools(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'fr_ir.py').write_text('def relay(value):\n    return value + 1\n')
            (root / 'evidence_basis.py').write_text('amount = 2\n')
            (root / 'sitecustomize.py').write_text("raise RuntimeError('project startup hook ran before the explicit import')\n")
            (root / 'question.py').write_text('import sys\nassert not any(name.startswith("fr_ir") for name in sys.modules)\nfrom fr_ir import relay\nfrom evidence_basis import amount\ndef entry():\n    return relay(emit_value()) + amount\n')
            request = root / 'request.json'
            request.write_text(json.dumps({'scope': str(root), 'path': 'question.py', 'entry': 'entry', 'value': 17}))
            result = subprocess.run([sys.executable, '-I', '-B', '-c', questions.RUNTIME_SOURCE, str(request)],
                                    cwd=root, capture_output=True, text=True, check=True, timeout=10)
            self.assertEqual(json.loads(result.stdout), {'value': 20})


class FrozenImplementation(unittest.TestCase):
    def fixture(self, root):
        def git(*args):
            return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, text=True, timeout=10).stdout.strip()
        def commit():
            git('add', '.')
            git('commit', '-qm', 'Record the source fixture')
        git('init', '-q')
        git('config', 'user.name', 'Fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        (root / 'src').mkdir()
        (root / 'src/lib.rs').write_text('pub fn value() -> i32 { 1 }\n')
        (root / 'native-source-inputs.txt').write_text('Cargo.toml\nCargo.lock\nnative-source-inputs.txt\nsrc/\n')
        (root / 'Cargo.toml').write_text('[workspace]\nmembers=["."]\n[package]\nname="fixture"\nversion="1.0.0"\n[dependencies]\nremote="2.0.0"\n')
        (root / 'Cargo.lock').write_text('version=4\n[[package]]\nname="fixture"\nversion="1.0.0"\n')
        sdk = root / 'sdk/python/src/fr_ir'
        sdk.mkdir(parents=True)
        (sdk / '_version.py').write_text('VERSION = "1.0.0"  # x-release-please-version\n')
        (sdk / 'flow.py').write_text('value = 1\n')
        commit()
        paths = ('Cargo.toml', 'Cargo.lock', 'native-source-inputs.txt', 'src', 'sdk/python/src')
        frozen = {'analyzer_revision': git('rev-parse', 'HEAD'),
                  'analyzer_trees': {name: git('rev-parse', 'HEAD:' + name) for name in paths}}
        return paths, frozen, commit

    def test_merge_release_labels_preserve_the_frozen_code(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths, frozen, commit = self.fixture(root)
            for name in ('Cargo.toml', 'Cargo.lock', 'sdk/python/src/fr_ir/_version.py'):
                path = root / name
                path.write_text(path.read_text().replace('1.0.0', '1.1.0'))
            commit()
            with patch.object(questions.flow, 'ANALYZER_PATHS', paths):
                questions.verify_analyzer(root, frozen)

    def test_code_dependencies_and_uncommitted_inputs_do_not_inherit_the_freeze(self):
        for mutation in ('native-code', 'sdk-code', 'dependency', 'version-code', 'uncommitted'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                paths, frozen, commit = self.fixture(root)
                if mutation in ('native-code', 'uncommitted'):
                    (root / 'src/lib.rs').write_text('pub fn value() -> i32 { 2 }\n')
                elif mutation == 'sdk-code':
                    (root / 'sdk/python/src/fr_ir/flow.py').write_text('value = 2\n')
                elif mutation == 'dependency':
                    path = root / 'Cargo.toml'
                    path.write_text(path.read_text().replace('2.0.0', '2.1.0'))
                elif mutation == 'version-code':
                    path = root / 'sdk/python/src/fr_ir/_version.py'
                    path.write_text(path.read_text() + 'extra_behavior = True\n')
                if mutation != 'uncommitted':
                    commit()
                with patch.object(questions.flow, 'ANALYZER_PATHS', paths):
                    with self.assertRaises(ValueError):
                        questions.verify_analyzer(root, frozen)


class CoverageBeforeUnread(unittest.TestCase):
    def example(self):
        record = {'head_sha': 'a' * 40, 'status': 'completed', 'conclusion': 'success',
                  'path': '.github/workflows/refinement-evidence.yml', 'event': 'workflow_dispatch',
                  'repository': {'full_name': 'example/project'}, 'head_repository': {'full_name': 'example/project'}}
        value = {'source_commit': 'a' * 40, 'phase': 'coverage', 'implementation': {'frozen': True},
                 'plan': copy.deepcopy(questions.PLAN), 'projects': []}
        for project in questions.PLAN['projects']:
            if project['role'] != 'coverage':
                continue
            row = {'project': copy.deepcopy(project), 'complete': True, 'error': None,
                   'original_sources_unchanged': True, 'cache': {'result': {'complete': True}, 'stages': []}, 'cases': []}
            previous = None
            for index, (label, reused, reaches) in enumerate(questions.CACHE_STAGES):
                manifest = str(index) * 64
                row['cache']['stages'].append({'exit_code': 0, 'stop_reason': None, 'result': {
                    'stage': label, 'reused': reused, 'reaches': reaches, 'complete': True,
                    'previous_manifest': previous, 'manifest': manifest}})
                previous = manifest
            value['projects'].append(row)
            for variant in ('original', 'renamed-relocated'):
                for case in ('positive', 'overwritten', 'unknown', 'steps'):
                    cell = {'variant': variant, 'case': case, 'analysis': {'exit_code': 0, 'stop_reason': None,
                            'result': {'complete': case in ('positive', 'overwritten'), 'witnesses': int(case == 'positive'),
                                       'cutoffs': [] if case in ('positive', 'overwritten') else ['unchecked']}}, 'runtime': []}
                    for index, item in enumerate(project['values']):
                        expected = project['blocked'] if case == 'overwritten' else project['expected'][index]
                        cell['runtime'].append({'input': item, 'expected': expected, 'observation':
                                                {'exit_code': 0, 'stop_reason': None, 'result': {'value': expected}}})
                    row['cases'].append(cell)
        return record, value

    def check(self, record, value):
        questions.checked_coverage(record, value, 'a' * 40, {'frozen': True}, 'example/project')

    def test_complete_declared_examples_allow_the_next_phase(self):
        self.check(*self.example())

    def test_missing_questions_wrong_answers_and_unfrozen_results_refuse(self):
        for mutation in ('revision', 'phase', 'freeze', 'empty', 'case', 'false-blocked', 'stop', 'runtime', 'runtime-type', 'expected', 'changed-source', 'cache', 'stage-link', 'stage-missing'):
            with self.subTest(mutation=mutation):
                record, value = self.example()
                row = value['projects'][0]
                if mutation == 'revision': record['head_sha'] = 'b' * 40
                elif mutation == 'phase': value['phase'] = 'development'
                elif mutation == 'freeze': value['implementation'] = None
                elif mutation == 'empty': value['projects'] = []
                elif mutation == 'case': row['cases'].pop()
                elif mutation == 'false-blocked': row['cases'][1]['analysis']['result']['witnesses'] = 1
                elif mutation == 'stop': row['cases'][0]['analysis']['stop_reason'] = 'cpu_seconds'
                elif mutation == 'runtime': row['cases'][0]['runtime'][0]['observation']['result']['value'] = 0
                elif mutation == 'runtime-type': row['cases'][0]['runtime'][0]['observation']['result']['value'] = float(row['cases'][0]['runtime'][0]['expected'])
                elif mutation == 'expected':
                    row['cases'][0]['runtime'][0]['expected'] = 0
                    row['cases'][0]['runtime'][0]['observation']['result']['value'] = 0
                elif mutation == 'changed-source': row['original_sources_unchanged'] = False
                elif mutation == 'cache': row['cache']['result']['complete'] = False
                elif mutation == 'stage-link': row['cache']['stages'][1]['result']['previous_manifest'] = 'f' * 64
                elif mutation == 'stage-missing': row['cache']['stages'].pop()
                with self.assertRaises(ValueError):
                    self.check(record, value)


class PublicQuestionChecks(unittest.TestCase):
    def example(self):
        project, = [project for project in questions.PLAN['projects'] if project['role'] == 'held-out']
        _, coverage = CoverageBeforeUnread().example()
        row = copy.deepcopy(coverage['projects'][0])
        row['project'] = copy.deepcopy(project)
        row.pop('cache')
        for cell in row['cases']:
            for index, item in enumerate(cell['runtime']):
                expected = project['blocked'] if cell['case'] == 'overwritten' else project['expected'][index]
                item.update(input=project['values'][index], expected=expected)
                item['observation']['result'] = {'value': expected}
        return project, row

    def test_complete_public_question_is_checked_without_source_selection(self):
        project, row = self.example()
        questions.checked_questions([row], {project['id']: project}, False)

    def test_missing_questions_changed_helpers_and_integer_booleans_cannot_close_the_milestone(self):
        for mutation in ('helper', 'missing', 'boolean', 'unknown', 'renamed'):
            with self.subTest(mutation=mutation):
                project, row = self.example()
                if mutation == 'helper': row['project']['function'] = 'simpler'
                elif mutation == 'missing': row['cases'].pop()
                elif mutation == 'boolean': row['cases'][0]['runtime'][0]['observation']['result']['value'] = 1
                elif mutation == 'unknown': row['cases'][2]['analysis']['result']['complete'] = True
                elif mutation == 'renamed': row['cases'][4]['analysis']['result']['witnesses'] = 0
                with self.assertRaises(ValueError):
                    questions.checked_questions([row], {project['id']: project}, False)


class CacheWorkerStops(unittest.TestCase):
    def test_stopped_workers_preserve_the_failure_and_restore_original_source(self):
        for stopped in ('helper-replaced', 'import-replaced'):
            with self.subTest(stage=stopped), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                scope, output = root / 'workspace', root / 'output'
                scope.mkdir()
                output.mkdir()
                (scope / 'library').mkdir()
                helper = scope / 'library/helper.py'
                original = b'def relay(value):\n    return value\n'
                helper.write_bytes(original)
                project = {'module': 'library.helper', 'function': 'relay', 'expression': 'relay(VALUE)'}
                row = {}
                collection = {'projects': [row]}
                seen = []

                def worker(_output, _name, request, _scope):
                    label = questions.CACHE_STAGES[request['stage']][0]
                    seen.append(label)
                    return {'exit_code': 125 if label == stopped else 0,
                            'stop_reason': 'cpu_seconds' if label == stopped else None,
                            'result': None if label == stopped else {'complete': True, 'manifest': str(request['stage']) * 64}}

                with patch.object(questions, 'execute', side_effect=worker):
                    with self.assertRaisesRegex(ValueError, 'resource budget: ' + stopped):
                        questions.collect_cache_controls(scope, project, output, Path('/unused/fr'), row, collection, output)
                self.assertEqual(helper.read_bytes(), original)
                self.assertFalse(helper.with_suffix('').exists())
                self.assertFalse(helper.with_suffix('.question-original').exists())
                self.assertEqual(seen[-1], stopped)
                self.assertEqual(row['cache']['stages'][-1]['stop_reason'], 'cpu_seconds')
                self.assertFalse(row['cache']['result']['complete'])


class MilestoneOutcome(unittest.TestCase):
    def test_three_independent_projects_include_one_unread_success_and_preserve_the_other_failure(self):
        familiar = [{'project': {'repository': name}, 'complete': True, 'error': None} for name in ('one', 'two')]
        rows = [{'project': {'repository': 'three'}, 'complete': True, 'error': None},
                {'project': {'repository': 'four'}, 'complete': False, 'error': 'no eligible helper'}]
        self.assertTrue(questions.milestone_outcome(rows, {'projects': familiar}, 'held-out', {'frozen': True})['milestones_complete'])
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[1]['error'], 'no eligible helper')
        for phase, frozen, coverage in [('development', None, None), ('held-out', None, {'projects': familiar}),
                                       ('held-out', {'frozen': True}, None)]:
            self.assertFalse(questions.milestone_outcome(rows, coverage, phase, frozen)['milestones_complete'])
        rows[0]['project']['repository'] = 'one'
        self.assertFalse(questions.milestone_outcome(rows, {'projects': familiar}, 'held-out', {'frozen': True})['milestones_complete'])
        rows[0]['complete'] = False
        self.assertFalse(questions.milestone_outcome(rows, {'projects': familiar}, 'held-out', {'frozen': True})['milestones_complete'])


if __name__ == '__main__':
    unittest.main()
