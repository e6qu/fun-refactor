"""Reject comparisons that omit startup work or change the user's question."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('process_cost', Path(__file__).with_name('python-process-cost.py'))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def fixture():
    sessions, answers = [], {}
    for case in cost.CASES:
        for repetition in range(5):
            for condition in cost.CONDITIONS:
                launches, requests = [], []
                for stage, (name, function, state) in enumerate(cost.STAGES):
                    wanted, files = cost.expected(case, stage), cost.sources(case, state)
                    inputs = {'source': {'path': 'app.py', 'digest': cost.digest(files['app.py'])},
                              'configuration': cost.digest({}), 'selection': {'name': function},
                              'budget': {'steps': cost.CASES[case]['steps'], 'depth': 8, 'bytes': 1_048_576},
                              'context': 'html', 'summary_mode': True}
                    if cost.CASES[case]['imports']:
                        inputs['modules'] = {'files': {path: cost.digest(text) for path, text in files.items()
                                                      if path.endswith('.py')}}
                    answer = {'complete': wanted['complete'], 'inputs': inputs,
                              'input_digest': cost.digest(inputs),
                              'witnesses': [17] if wanted['python']['returned'] == 17 else [],
                              'cutoffs': [] if wanted['complete'] else [
                                  'step-budget' if case == 'unfinished-budget' else 'module-effects-unchecked']}
                    key = cost.digest(answer)
                    answers[key] = answer
                    requests.append({'stage': name, 'answer_digest': key, 'seconds': 0.02,
                                     'body_seconds': 0.015, 'analysis_steps': 1,
                                     'calls': [{'operation': operation, 'seconds': 0.005}
                                               for operation in ('find', 'dataflow')]})
                    if stage == 0 or condition.endswith('fresh'):
                        launches.append({'ready': True, 'stopped': True, 'startup_seconds': 0.1,
                                         'shutdown_seconds': 0.01, 'sdk_copy': condition.split('-')[0],
                                         'bytecode': condition.startswith('installed'),
                                         'module_files': ['fr_ir/runtime.py'],
                                         'timing': {'base_import_seconds': 0.01, 'arguments_seconds': 0.001,
                                                    'sdk_import_seconds': 0.05, 'bootstrap_seconds': 0.07}})
                total = sum(item['startup_seconds'] + item['shutdown_seconds'] for item in launches)
                total += sum(item['seconds'] for item in requests)
                sessions.append({'case': case, 'condition': condition, 'repetition': repetition,
                                 'position': (cost.CONDITIONS.index(condition) - repetition) % 4,
                                 'launches': launches, 'requests': requests, 'total_seconds': total,
                                 'wall_seconds': total + 0.01, 'initial_cache_files': 0,
                                 'cache_files': 3, 'cache_bytes': 100})
    files = {path.relative_to(cost.ROOT / 'sdk/python/src').as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
             for path in (cost.ROOT / 'sdk/python/src/fr_ir').rglob('*.py')}
    return {'schema': 'fr-python-process-cost', 'bindings': {'source': 'frozen'}, 'scope': cost.SCOPE,
            'cases': cost.CASES, 'stages': cost.json.loads(cost.encode(cost.STAGES)),
            'repetitions': 5, 'conditions': list(cost.CONDITIONS), 'environment': {'build': cost.BUILD},
            'binary_sha256': 'a' * 64, 'sdk_files': {'source': files, 'installed': files.copy()},
            'setup_seconds': dict.fromkeys(('wheel_build', 'wheel_install', 'bytecode_compile'), 0.1),
            'sessions': sessions, 'answers': answers, 'summary': cost.summary(sessions)}


class Controls(unittest.TestCase):
    def setUp(self):
        self.report = fixture()
        self.binding = patch.object(cost, 'bindings', return_value={'source': 'frozen'})
        self.binding.start()
        self.addCleanup(self.binding.stop)

    def reject(self, change, message):
        changed = copy.deepcopy(self.report)
        change(changed)
        with self.assertRaisesRegex(ValueError, message):
            cost.audit(changed)

    def test_complete_comparison_passes(self):
        cost.audit(self.report)

    def test_missing_and_duplicate_sessions_fail(self):
        self.reject(lambda r: r['sessions'].pop(), 'sessions')
        self.reject(lambda r: r['sessions'].__setitem__(1, r['sessions'][0]), 'duplicate')

    def test_source_and_installed_code_must_match(self):
        self.reject(lambda r: r['sdk_files']['installed'].pop('fr_ir/runtime.py'), 'copies differ')
        def change_both(r):
            for files in r['sdk_files'].values():
                files['fr_ir/runtime.py'] = 'b' * 64
        self.reject(change_both, 'source receipt')

    def test_installation_cost_cannot_disappear(self):
        self.reject(lambda r: r['setup_seconds'].pop('bytecode_compile'), 'installation cost')

    def test_unaccounted_process_launch_fails(self):
        self.reject(lambda r: r['sessions'][0]['launches'].pop(), 'process count')

    def test_source_cannot_claim_installed_bytecode(self):
        self.reject(lambda r: r['sessions'][0]['launches'][0].update(bytecode=True), 'import receipt')

    def test_import_timing_must_fit_parent_startup(self):
        self.reject(lambda r: r['sessions'][0]['launches'][0].update(startup_seconds=0.01), 'timing boundaries')

    def test_startup_cannot_disappear_from_total(self):
        self.reject(lambda r: r['sessions'][0].update(total_seconds=0.14), 'omits work')

    def test_wall_time_must_cover_measured_work(self):
        self.reject(lambda r: r['sessions'][0].update(wall_seconds=0), 'omits work')

    def test_condition_order_is_visible(self):
        self.reject(lambda r: r['sessions'][0].update(position=3), 'order changed')

    def test_native_requests_cannot_disappear(self):
        self.reject(lambda r: r['sessions'][0]['requests'][0]['calls'].pop(), 'native work')

    def test_invalid_and_fractional_counts_fail(self):
        for value in (-1, float('nan'), float('inf'), True):
            self.reject(lambda r: r['sessions'][0]['requests'][0].update(seconds=value), 'invalid request')
        self.reject(lambda r: r['sessions'][0]['requests'][0].update(analysis_steps=0.5), 'analysis work')

    def test_primed_project_cache_fails(self):
        self.reject(lambda r: r['sessions'][0].update(initial_cache_files=3), 'primed')
        self.reject(lambda r: r['sessions'][0].update(cache_files=0), 'did not populate')

    def test_missing_question_and_changed_order_fail(self):
        self.reject(lambda r: r['sessions'][0]['requests'].pop(), 'question missing')
        self.reject(lambda r: r['sessions'][0]['requests'].reverse(), 'question order')

    def test_settings_and_source_edits_cannot_change(self):
        def change(r):
            row = r['sessions'][0]['requests'][3]
            answer = copy.deepcopy(r['answers'][row['answer_digest']])
            answer['inputs']['budget']['steps'] = 9999
            answer['input_digest'] = cost.digest(answer['inputs'])
            row['answer_digest'] = cost.digest(answer)
            r['answers'][row['answer_digest']] = answer
        self.reject(change, 'analysis settings')
        self.reject(lambda r: r['bindings'].update(source='changed'), 'sources changed')

    def test_all_conditions_must_return_the_same_full_answer(self):
        def change(r):
            row = r['sessions'][1]['requests'][0]
            answer = copy.deepcopy(r['answers'][row['answer_digest']])
            answer['other_semantics'] = 'changed'
            row['answer_digest'] = cost.digest(answer)
            r['answers'][row['answer_digest']] = answer
        self.reject(change, 'changed answers')

    def test_unknown_cannot_become_complete(self):
        def change(r):
            row = r['sessions'][0]['requests'][4]
            answer = copy.deepcopy(r['answers'][row['answer_digest']])
            answer['complete'] = True
            row['answer_digest'] = cost.digest(answer)
            r['answers'][row['answer_digest']] = answer
        self.reject(change, 'completion changed')

    def test_checked_hash_bytecode_is_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'fr_ir/__init__.py'
            path.parent.mkdir()
            path.write_text('VALUE = 17\n')
            cost.sdk_inventory(root, False)
            import py_compile
            py_compile.compile(str(path), doraise=True,
                               invalidation_mode=py_compile.PycInvalidationMode.CHECKED_HASH)
            cost.sdk_inventory(root, True)
            with self.assertRaisesRegex(ValueError, 'already has'):
                cost.sdk_inventory(root, False)
            path.write_text('VALUE = 0\n')
            with self.assertRaisesRegex(ValueError, 'stale'):
                cost.sdk_inventory(root, True)

    def test_installed_worker_cannot_load_repository_source(self):
        with self.assertRaisesRegex(ValueError, 'different SDK copy'):
            cost.verify_modules({name: {'file': '/repository/fr_ir/runtime.py', 'cached': ''}
                                 for name in ('fr_ir', 'fr_ir.flow', 'fr_ir.runtime')},
                                Path('/installed'), {}, True)


if __name__ == '__main__':
    unittest.main()
