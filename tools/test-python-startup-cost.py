"""Reject incomplete, inconsistent and misleading saved cost comparisons."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('startup_cost', Path(__file__).with_name('python-startup-cost.py'))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def fixture(engine, cache_mode, repetitions):
    rows, answers, observations = [], {}, []
    for case in cost.CASES:
        for stage, (name, function, state) in enumerate(cost.STAGES):
            wanted = cost.expected(case, stage)
            files = cost.sources(case, state)
            inputs = {'source': {'path': 'app.py', 'digest': cost.digest(files['app.py'])},
                      'configuration': cost.digest({}), 'selection': {'name': function},
                      'budget': {'steps': cost.CASES[case]['steps'], 'depth': 8, 'bytes': 1_048_576},
                      'context': 'html', 'summary_mode': True}
            if cost.CASES[case]['imports']:
                inputs['modules'] = {'files': {name: cost.digest(contents) for name, contents in files.items()
                                              if name.endswith('.py')}}
            answer = {'case': case, 'stage': name, 'complete': wanted['complete'],
                      'inputs': inputs, 'input_digest': cost.digest(inputs),
                      'witnesses': [17] if wanted['python']['returned'] == 17 else [],
                      'cutoffs': [] if wanted['complete'] else [
                          'step-budget' if case == 'unfinished-budget' else 'module-effects-unchecked']}
            key = cost.digest(answer)
            answers[key] = answer
            observations.append({'case': case, 'stage': name, 'result': wanted['python'], 'elapsed_seconds': 0.01})
            for repetition in range(repetitions):
                for strategy in cost.STRATEGIES:
                    metrics = dict.fromkeys(cost.FIELDS, 0)
                    operations = ['find', 'inputs', 'answer'] if strategy == 'saved-answer' else ['find', 'answer']
                    calls = []
                    for position, operation in enumerate(operations):
                        profile = None
                        if engine == 'profile':
                            hits = 0 if stage == 0 and position == 0 else 1
                            if stage in (3, 4) and position == 0:
                                hits = 0
                            profile = {'schema': 'fr-project-profile-1',
                                       'phases_seconds': dict.fromkeys(cost.PHASES, 0.0001),
                                       'construction_seconds': dict.fromkeys(cost.CONSTRUCTION, 0.00001),
                                       'measured_seconds': 0.001, 'indexed_files': 1,
                                       'fact_cache_hits': hits if cache_mode == 'enabled' else None,
                                       'resolution_cache_hits': 0 if cache_mode == 'enabled' else None,
                                       'resolution_cache_misses': 0 if cache_mode == 'enabled' else None}
                        calls.append({'operation': operation, 'seconds': 0.002, 'reply_bytes': 100, 'profile': profile})
                    metrics.update(elapsed_seconds=0.03, worker_seconds=0.02,
                                   native_calls=len(calls), native_seconds=sum(call['seconds'] for call in calls),
                                   canonical_reply_bytes=100 * len(calls),
                                   find_seconds=0.002, inputs_seconds=0.002 if strategy == 'saved-answer' else 0,
                                   answer_seconds=0.002)
                    if cache_mode == 'enabled':
                        metrics.update(project_cache_files=stage + 1, project_cache_bytes=100 * (stage + 1),
                                       project_cache_added_files=1, project_cache_added_bytes=100)
                    rows.append({'case': case, 'stage': name, 'repetition': repetition, 'strategy': strategy,
                                 'position': (cost.STRATEGIES.index(strategy) - repetition) % 3,
                                 'cache_position': (cost.CACHE_MODES.index(cache_mode) - repetition) % 2,
                                 'answer_digest': key, 'startup_checks': int(not wanted['startup_reused'])
                                 if strategy == 'saved-startup' else None,
                                 'startup_reused': strategy == 'saved-startup' and wanted['startup_reused'],
                                 'answer_reused': strategy == 'saved-answer' and wanted['answer_reused'],
                                 'calls': calls, 'metrics': metrics})
    return {'schema': 'fr-python-startup-cost', 'bindings': {'source': 'frozen'}, 'scope': cost.SCOPE,
            'cases': cost.CASES, 'stages': cost.STAGES, 'strategies': cost.STRATEGIES, 'repetitions': repetitions,
            'engine': engine, 'project_cache': cache_mode, 'environment': {'build': cost.BUILD},
            'binary_sha256': 'a' * 64,
            'samples': rows, 'answers': answers, 'observations': observations, 'summary': cost.summarize(rows),
            'sequence_totals': cost.sequence_totals(rows, repetitions)}


class Controls(unittest.TestCase):
    def setUp(self):
        self.report = fixture('cli', 'disabled', 3)
        self.binding = patch.object(cost, 'bindings', return_value={'source': 'frozen'})
        self.binding.start()
        self.addCleanup(self.binding.stop)

    def check(self, change):
        changed = copy.deepcopy(self.report)
        change(changed)
        changed['summary'] = cost.summarize(changed['samples'])
        changed['sequence_totals'] = cost.sequence_totals(changed['samples'], 3)
        with self.assertRaises(ValueError):
            cost.audit(changed)

    def test_complete_control_is_accepted(self):
        for engine in cost.ENGINES:
            for mode in cost.CACHE_MODES:
                with self.subTest(engine=engine, cache_mode=mode):
                    cost.audit(fixture(engine, mode, 3))

    def comparison(self, change):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reports = {f'{engine}-{mode}': fixture(engine, mode, 5 if engine == 'cli' else 3)
                       for engine in cost.ENGINES for mode in cost.CACHE_MODES}
            change(reports)
            for name, report in reports.items():
                (root / name).mkdir()
                (root / name / 'result.json').write_text(json.dumps(report))
            cost.audit_set(root)

    def test_complete_cross_condition_comparison_is_accepted(self):
        self.comparison(lambda reports: None)

    def test_cross_condition_binary_and_environment_changes_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'different binaries'):
            self.comparison(lambda reports: reports['cli-enabled'].update(binary_sha256='b' * 64))
        with self.assertRaisesRegex(ValueError, 'different environments'):
            self.comparison(lambda reports: reports['profile-enabled']['environment'].update(cpu_count=99))

    def test_profiler_and_cache_modes_must_return_the_same_answers(self):
        def change(reports):
            report = reports['profile-enabled']
            renamed = {}
            for old, answer in report['answers'].items():
                answer['extra'] = 'profile changed an answer'
                renamed[old] = cost.digest(answer)
            report['answers'] = {renamed[old]: answer for old, answer in report['answers'].items()}
            for row in report['samples']:
                row['answer_digest'] = renamed[row['answer_digest']]
        with self.assertRaisesRegex(ValueError, 'changed answers'):
            self.comparison(change)

    def test_execution_settings_are_explicit(self):
        self.check(lambda report: report.update(project_cache=None))
        self.check(lambda report: report.update(engine=None))

    def test_cache_order_cannot_be_hidden(self):
        self.check(lambda report: report['samples'][0].update(cache_position=1))

    def test_fractional_counts_are_rejected(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(analysis_steps=0.5))

    def test_each_native_operation_is_accounted_for(self):
        self.check(lambda report: report['samples'][0]['calls'].pop())
        self.check(lambda report: report['samples'][0]['calls'][0].update(operation='answer'))
        self.check(lambda report: report['samples'][0]['metrics'].update(find_seconds=0))
        self.check(lambda report: report['samples'][0]['metrics'].update(canonical_reply_bytes=0))

    def test_disabled_project_cache_cannot_write_files(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(project_cache_files=1))

    def test_initial_project_cache_population_cannot_be_excluded(self):
        self.report = fixture('cli', 'enabled', 3)
        self.check(lambda report: report['samples'][0]['metrics'].update(project_cache_added_bytes=0))

    def test_enabled_cache_cannot_silently_be_disabled(self):
        self.report = fixture('cli', 'enabled', 3)
        self.check(lambda report: report['samples'][0]['metrics'].update(project_cache_files=0))

    def test_profiling_cannot_be_mixed_into_cli_time(self):
        self.check(lambda report: report['samples'][0]['calls'][0].update(profile={}))

    def test_diagnostic_phase_totals_are_bounded(self):
        self.report = fixture('profile', 'enabled', 3)
        self.check(lambda report: report['samples'][0]['calls'][0]['profile']['phases_seconds'].update(index=1))
        self.check(lambda report: report['samples'][0]['calls'][0]['profile']['construction_seconds'].update(setup=1))
        self.check(lambda report: report['samples'][0]['calls'][0]['profile']['phases_seconds'].update(index=-1))

    def test_project_cache_priming_and_stale_hits_are_rejected(self):
        self.report = fixture('profile', 'enabled', 3)
        self.check(lambda report: report['samples'][0]['calls'][0]['profile'].update(fact_cache_hits=1))
        edited = next(i for i, row in enumerate(self.report['samples']) if row['stage'] == 'edited-helper')
        self.check(lambda report: report['samples'][edited]['calls'][0]['profile'].update(fact_cache_hits=1))

    def test_unchanged_files_must_use_the_enabled_cache(self):
        self.report = fixture('profile', 'enabled', 3)
        self.check(lambda report: report['samples'][0]['calls'][1]['profile'].update(fact_cache_hits=0))

    def test_profile_counters_cannot_hide_disabled_cache_use(self):
        self.report = fixture('profile', 'disabled', 3)
        self.check(lambda report: report['samples'][0]['calls'][0]['profile'].update(fact_cache_hits=0))

    def test_all_rows_are_required(self):
        self.check(lambda report: report['samples'].pop())

    def test_duplicate_requests_are_rejected(self):
        self.check(lambda report: report['samples'].__setitem__(1, report['samples'][0]))

    def test_source_edit_requires_new_collection(self):
        self.check(lambda report: report['bindings'].update(source='changed'))

    def test_strategy_order_cannot_be_hidden(self):
        self.check(lambda report: report['samples'][0].update(position=2))

    def test_whole_answer_preflight_is_counted(self):
        self.check(lambda report: report['samples'][2]['metrics'].update(native_calls=2))

    def test_fresh_baseline_cannot_read_a_store(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(store_gets=1))

    def test_invalid_timing_is_rejected(self):
        for value in (-1, float('inf'), float('nan'), True):
            with self.subTest(value=value):
                self.check(lambda report: report['samples'][0]['metrics'].update(elapsed_seconds=value))

    def test_hidden_extra_time_is_rejected(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(native_seconds=0.1))

    def test_budget_cannot_increase(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(analysis_steps=4097))

    def test_analysis_settings_cannot_be_substituted(self):
        def change(report):
            row = report['samples'][0]
            answer = report['answers'].pop(row['answer_digest'])
            answer['inputs']['budget']['steps'] = 2048
            answer['input_digest'] = cost.digest(answer['inputs'])
            key = cost.digest(answer)
            report['answers'][key] = answer
            previous = row['answer_digest']
            for other in report['samples']:
                if other['answer_digest'] == previous:
                    other['answer_digest'] = key
        self.check(change)

    def test_reuse_cannot_be_invented(self):
        self.check(lambda report: report['samples'][0].update(startup_reused=True))

    def test_completion_cannot_be_invented(self):
        def change(report):
            row = next(row for row in report['samples'] if row['case'] == 'unfinished-budget')
            answer = report['answers'][row['answer_digest']]
            answer['complete'] = True
        self.check(change)

    def test_saved_and_fresh_answers_must_agree(self):
        def change(report):
            row = report['samples'][1]
            answer = copy.deepcopy(report['answers'][row['answer_digest']])
            answer['extra'] = 'different'
            key = cost.digest(answer)
            report['answers'][key] = answer
            row['answer_digest'] = key
        self.check(change)

    def test_python_failures_cannot_be_erased(self):
        def change(report):
            row = next(row for row in report['observations'] if row['stage'] == 'unknown-startup')
            row['result'] = {'error': None, 'returned': 17}
        self.check(change)

    def test_python_observations_cannot_be_duplicated(self):
        self.check(lambda report: report['observations'].__setitem__(1, report['observations'][0]))

    def test_summary_cannot_be_edited_independently(self):
        self.report['summary']['local']['first-question']['fresh']['elapsed_seconds']['median'] = 0.001
        with self.assertRaises(ValueError):
            cost.audit(self.report)

    def test_semantic_normalization_preserves_everything_except_execution_metadata(self):
        original = {'execution': {'kind': 'analyzed'}, 'context_basis': 'basis',
                    'complete': False, 'cutoffs': ['unknown'], 'witnesses': [], 'revision': 'current'}
        normalized = cost.evidence(original)
        self.assertEqual(normalized, {'complete': False, 'cutoffs': ['unknown'],
                                      'witnesses': [], 'revision': 'current'})
        self.assertIn('execution', original)


if __name__ == '__main__':
    unittest.main()
