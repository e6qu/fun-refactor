"""Protect the unread helper selection and retain an empty selection as a failure."""
import importlib.util
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_questions', ROOT / 'tools/package-questions.py')
questions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(questions)


class UnreadSelection(unittest.TestCase):
    def test_selection_uses_path_then_declaration_order(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'z.py').write_text('def z(value):\n    return value\n')
            (root / 'a.py').write_text('def first(item):\n    """User docs."""\n    return item\ndef second(value):\n    return value\n')
            selected = questions.inspect_unread(root)
            self.assertEqual((selected['module'], selected['function'], selected['line']), ('a', 'first', 1))
            self.assertEqual(selected['source_sha256'], questions.flow.sha((root / 'a.py').read_bytes()))

    def test_copied_examples_decorators_methods_and_nonidentity_bodies_do_not_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'tests').mkdir()
            (root / 'tests/identity.py').write_text('def identity(value):\n    return value\n')
            (root / 'library.py').write_text('''@replace
def decorated(value):
    return value
def arithmetic(value):
    return value + 1
def two(value, other):
    return value
class Wrapper:
    def method(value):
        return value
''')
            with self.assertRaisesRegex(ValueError, 'no helper satisfies'):
                questions.inspect_unread(root)

    def test_package_initializer_keeps_the_original_import(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'library').mkdir()
            (root / 'library/__init__.py').write_text('def identity(value):\n    return value\n')
            selected = questions.inspect_unread(root)
            self.assertEqual(selected['module'], 'library')
            self.assertEqual(selected['path'], 'library/__init__.py')


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
                   'original_sources_unchanged': True, 'cache': {'result': {'complete': True}}, 'cases': []}
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
        with patch.dict(questions.os.environ, {'GITHUB_REPOSITORY': 'example/project'}):
            questions.checked_coverage(record, value, 'a' * 40, {'frozen': True})

    def test_complete_declared_examples_allow_the_next_phase(self):
        self.check(*self.example())

    def test_missing_questions_wrong_answers_and_unfrozen_results_refuse(self):
        for mutation in ('revision', 'phase', 'freeze', 'empty', 'case', 'false-blocked', 'stop', 'runtime', 'expected', 'changed-source', 'cache'):
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
                elif mutation == 'expected':
                    row['cases'][0]['runtime'][0]['expected'] = 0
                    row['cases'][0]['runtime'][0]['observation']['result']['value'] = 0
                elif mutation == 'changed-source': row['original_sources_unchanged'] = False
                elif mutation == 'cache': row['cache']['result']['complete'] = False
                with self.assertRaises(ValueError):
                    self.check(record, value)


if __name__ == '__main__':
    unittest.main()
