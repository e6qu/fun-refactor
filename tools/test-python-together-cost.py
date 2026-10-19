"""Reject comparisons that hide shared limits, changed answers or setup costs."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('together_cost', Path(__file__).with_name('python-together-cost.py'))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def fixture():
    sessions, reports = [], {}
    for case in cost.CASES:
        steps = cost.CASES[case]['steps']
        for repetition in range(5):
            for condition in cost.CONDITIONS:
                requests = []
                for label, state, names in cost.STAGES:
                    answers = []
                    lookups = []
                    files = cost.process.sources(case, state)
                    for index, name in enumerate(names):
                        inputs = {'source': {'path': 'app.py', 'digest': cost.digest(files['app.py'])},
                                  'configuration': cost.digest({}), 'selection': {'name': name},
                                  'rules': cost.digest(cost.json.loads(files['rules.json']) | {'propagators': []}),
                                  'budget': {'steps': steps, 'depth': 8, 'bytes': 1_048_576},
                                  'context': 'html', 'summary_mode': True}
                        if cost.CASES[case]['imports']:
                            inputs['modules'] = {'files': {path: cost.digest(text) for path, text in files.items()
                                                          if path.endswith('.py')}}
                        complete = case != 'unfinished-budget' and state != 'unknown'
                        work = 0 if case == 'unfinished-budget' and condition == 'together' and index == 1 else 1
                        revision = 'a' * 64
                        target = 'frp1:' + revision[:32] + ':' + ('1' if name == 'forwards' else '2')
                        lookups.append({'schema': 'fr-project-1', 'query': 'find', 'revision': revision,
                                        'columns': ['handle', 'name', 'path'], 'rows': [[target, name, 'app.py']],
                                        'page': {'remaining': 0}})
                        answers.append({'schema': 'fr-dataflow-1', 'inputs': inputs,
                                        'revision': revision, 'target': target,
                                        'input_digest': cost.digest(inputs), 'complete': complete,
                                        'budget': {'steps': steps, 'used': work, 'response_bytes': 1_048_576},
                                        'witnesses': [17] if name == 'forwards' or state == 'edited' else [],
                                        'cutoffs': [] if complete else [
                                            'step-budget' if case == 'unfinished-budget' else 'module-effects-unchecked'],
                                        'execution': {'kind': 'analyzed', 'analysis_steps': work}, 'context_basis': {}})
                    if condition == 'together':
                        shared = dict(inputs)
                        del shared['selection']
                        raw = [{'schema': 'fr-dataflow-together', 'answers': answers,
                                'startup': {'checks': 1, 'questions': 2, 'inputs': shared,
                                            'input_digest': cost.digest(shared)},
                                'budget': {'steps': steps, 'used': sum(a['execution']['analysis_steps'] for a in answers),
                                           'response_bytes': 1_048_576}}]
                    else:
                        raw = answers
                    key = cost.digest(raw)
                    reports[key] = raw
                    calls = [
                                 {'operation': 'find' if index < 2 else 'dataflow', 'seconds': 0.005,
                                  'report_json_bytes': len(cost.json.dumps(value, ensure_ascii=False,
                                                                          separators=(',', ':')).encode())}
                                 for index, value in enumerate(lookups + raw)]
                    requests.append({'stage': label, 'names': names, 'report_digest': key,
                                     'seconds': 0.03, 'body_seconds': 0.025,
                                     'calls': calls, 'lookups': lookups})
                launch = {'ready': True, 'stopped': True, 'startup_seconds': 0.1,
                          'bootstrap_seconds': 0.08, 'shutdown_seconds': 0.01,
                          'module_files': ['fr_ir/__init__.py', 'fr_ir/runtime.py', 'fr_ir/flow.py',
                                           'fr_ir/flow_together.py']}
                total = launch['startup_seconds'] + launch['shutdown_seconds'] + sum(r['seconds'] for r in requests)
                sessions.append({'case': case, 'condition': condition, 'repetition': repetition,
                                 'position': (cost.CONDITIONS.index(condition) - repetition) % 2,
                                 'launch': launch, 'requests': requests, 'total_seconds': total,
                                 'wall_seconds': total + 0.01, 'initial_cache_files': 0,
                                 'cache_files': 3, 'cache_bytes': 100})
    files = {path.relative_to(cost.ROOT / 'sdk/python/src').as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
             for path in (cost.ROOT / 'sdk/python/src/fr_ir').rglob('*.py')}
    observations = {case: {state: ({'error': 'NameError', 'returned': None} if state == 'unknown'
                                  else {'error': None, 'returned': [17, 17 if state == 'edited' else 0]})
                          for state in ('original', 'edited', 'unknown')} for case in cost.CASES}
    return {'schema': 'fr-python-together-cost', 'bindings': {'source': 'frozen'}, 'scope': cost.SCOPE,
            'cases': cost.CASES, 'stages': cost.json.loads(cost.encode(cost.STAGES)),
            'conditions': list(cost.CONDITIONS), 'repetitions': 5, 'binary_sha256': 'a' * 64,
            'wheel_sha256': 'b' * 64, 'environment': {'build': cost.process.BUILD},
            'sdk_files': {'source': files, 'installed': files.copy()},
            'setup_seconds': dict.fromkeys(('wheel_build', 'wheel_install', 'bytecode_compile'), 0.1),
            'python_observations': observations, 'sessions': sessions, 'reports': reports,
            'summary': cost.summary(sessions)}


def change_raw(report, row, change):
    raw = copy.deepcopy(report['reports'][row['report_digest']])
    change(raw)
    key = cost.digest(raw)
    report['reports'][key] = raw
    row['report_digest'] = key
    for call, value in zip(row['calls'][2:], raw):
        call['report_json_bytes'] = len(cost.json.dumps(value, ensure_ascii=False,
                                                       separators=(',', ':')).encode())


class Controls(unittest.TestCase):
    def setUp(self):
        self.report = fixture()
        binding = patch.object(cost, 'bindings', return_value={'source': 'frozen'})
        binding.start()
        self.addCleanup(binding.stop)

    def reject(self, change, message):
        report = copy.deepcopy(self.report)
        change(report)
        with self.assertRaisesRegex(ValueError, message):
            cost.audit(report)

    def test_complete_comparison_passes(self):
        cost.audit(self.report)

    def test_missing_duplicate_and_reordered_sessions_fail(self):
        self.reject(lambda r: r['sessions'].pop(), 'sessions')
        self.reject(lambda r: r['sessions'].__setitem__(1, r['sessions'][0]), 'duplicate')
        self.reject(lambda r: r['sessions'][0].update(position=1), 'order')

    def test_changed_source_or_installed_sdk_fails(self):
        self.reject(lambda r: r['bindings'].update(source='changed'), 'sources')
        self.reject(lambda r: r['sdk_files']['installed'].pop('fr_ir/flow_together.py'), 'SDK source')

    def test_missing_install_and_import_costs_fail(self):
        self.reject(lambda r: r['setup_seconds'].pop('wheel_install'), 'installation')
        self.reject(lambda r: r['sessions'][0]['launch'].update(startup_seconds=0), 'startup')
        self.reject(lambda r: r['sessions'][0]['launch']['module_files'].remove('fr_ir/flow_together.py'), 'receipt')

    def test_initial_cache_fill_cannot_be_hidden(self):
        self.reject(lambda r: r['sessions'][0].update(initial_cache_files=1), 'cache')
        self.reject(lambda r: r['sessions'][0].update(cache_files=0), 'cache')

    def test_missing_question_or_native_call_fails(self):
        self.reject(lambda r: r['sessions'][0]['requests'].pop(), 'missing pair')
        self.reject(lambda r: r['sessions'][0]['requests'][0]['calls'].pop(), 'native calls')
        self.reject(lambda r: r['sessions'][0]['requests'][0]['calls'][0].update(report_json_bytes=0), 'call cost')

    def test_impossible_timings_and_unaccounted_totals_fail(self):
        self.reject(lambda r: r['sessions'][0]['requests'][0].update(body_seconds=0), 'timing')
        self.reject(lambda r: r['sessions'][0].update(total_seconds=0), 'omits work')
        self.reject(lambda r: r['sessions'][0].update(wall_seconds=0), 'omits work')
        self.reject(lambda r: r['sessions'][0]['requests'][0].update(seconds=float('nan')), 'timing')

    def test_python_behavior_and_source_edits_are_checked(self):
        self.reject(lambda r: r['python_observations']['local']['edited'].update(returned=[17, 0]), 'Python behavior')
        def change(r):
            row = r['sessions'][0]['requests'][3]
            change_raw(r, row, lambda raw: raw[0]['inputs']['source'].update(digest='0' * 64))
        self.reject(change, 'source or explicit')

    def test_equal_answers_cannot_quietly_diverge(self):
        def change(r):
            row = r['sessions'][1]['requests'][0]
            change_raw(r, row, lambda raw: raw[0]['answers'][0].update(extra_claim='changed'))
        self.reject(change, 'answers changed')

    def test_unknown_startup_cannot_become_complete(self):
        def change(r):
            row = r['sessions'][0]['requests'][4]
            change_raw(r, row, lambda raw: raw[0].update(complete=True))
        self.reject(change, 'unfinished answer')

    def test_shared_startup_and_budget_cannot_disappear(self):
        def change(r):
            row = r['sessions'][1]['requests'][0]
            change_raw(r, row, lambda raw: raw[0]['startup'].update(checks=2))
        self.reject(change, 'startup receipt')
        def change_budget(r):
            row = r['sessions'][1]['requests'][0]
            change_raw(r, row, lambda raw: raw[0]['budget'].update(steps=8192))
        self.reject(change_budget, 'shared work')

    def test_exhausted_second_question_cannot_claim_more_work(self):
        def change(r):
            row = next(row for row in r['sessions'] if row['case'] == 'unfinished-budget'
                       and row['condition'] == 'together')['requests'][0]
            def alter(raw):
                raw[0]['answers'][1]['execution']['analysis_steps'] = 1
                raw[0]['answers'][1]['budget']['used'] = 1
                raw[0]['budget']['used'] = 2
            change_raw(r, row, alter)
        self.reject(change, 'shared work')

    def test_changed_limits_and_native_byte_counts_fail(self):
        self.reject(lambda r: r['sessions'][0]['requests'][0]['calls'][2].update(report_json_bytes=1),
                    'report bytes')
        def change(r):
            row = r['sessions'][0]['requests'][0]
            change_raw(r, row, lambda raw: raw[0]['inputs']['budget'].update(bytes=2_097_152))
        self.reject(change, 'source or explicit')

    def test_search_receipts_must_select_the_analyzed_functions(self):
        self.reject(lambda r: r['sessions'][0]['requests'][0]['lookups'].pop(), 'search receipts')
        self.reject(lambda r: r['sessions'][0]['requests'][0]['lookups'][0]['rows'][0].__setitem__(1, 'other'),
                    'selected function')


if __name__ == '__main__':
    unittest.main()
