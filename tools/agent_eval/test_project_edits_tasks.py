"""Keep the M2 task requests separate from the implementation being evaluated."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import project_edits_tasks as tasks


class ProjectEditBoundaries(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def prepare(self, task):
        project = self.root / task
        record = tasks.prepare(task, project, self.root / (task + '-preparation'))
        return project, record

    def test_rust_parameter_change_keeps_comments_and_other_source_bytes(self):
        project, record = self.prepare('rust-parameter')
        tasks.prescribed_change('rust-parameter', 'reference', project)
        self.assertEqual(len(tasks.boundaries('rust-parameter', project, record)), 3)
        path = project / 'pricing/src/lib.rs'
        path.write_text(path.read_text().replace('café', 'cafe'))
        with self.assertRaisesRegex(ValueError, 'outside the parameter'):
            tasks.boundaries('rust-parameter', project, record)

    def test_typescript_export_and_alias_change_leave_the_unrelated_name_intact(self):
        project, record = self.prepare('typescript-alias')
        with self.assertRaisesRegex(ValueError, 'export/import'):
            tasks.boundaries('typescript-alias', project, record)
        tasks.prescribed_change('typescript-alias', 'reference', project)
        self.assertEqual(tasks.boundaries('typescript-alias', project, record), ['src/checkout.ts', 'src/pricing.ts'])
        path = project / 'src/unrelated.ts'
        path.write_text(path.read_text().replace('price', 'quote'))
        with self.assertRaisesRegex(ValueError, 'unrelated'):
            tasks.boundaries('typescript-alias', project, record)

    def test_python_body_can_change_without_changing_tests_or_the_other_function(self):
        project, record = self.prepare('python-invoice')
        tasks.prescribed_change('python-invoice', 'reference', project)
        self.assertEqual(tasks.boundaries('python-invoice', project, record), ['invoice.py'])
        path = project / 'test_invoice.py'
        path.write_text(path.read_text().replace('== 7', '== 5'))
        with self.assertRaisesRegex(ValueError, 'tests'):
            tasks.boundaries('python-invoice', project, record)

    def test_python_signature_and_surrounding_code_stay_fixed(self):
        project, record = self.prepare('python-invoice')
        path = project / 'invoice.py'
        path.write_text(path.read_text().replace('fee: int', 'fee: int = 0'))
        with self.assertRaisesRegex(ValueError, 'signature'):
            tasks.boundaries('python-invoice', project, record)

    def test_python_body_region_cannot_add_a_module_level_statement(self):
        project, record = self.prepare('python-invoice')
        path = project / 'invoice.py'
        path.write_text(path.read_text().replace('    return amount\n', '    return amount + fee\n\nextra = 1\n'))
        with self.assertRaisesRegex(ValueError, 'surrounding bytes'):
            tasks.boundaries('python-invoice', project, record)

    def test_explicit_model_and_checker_requirements_retain_48_slots(self):
        specification = tasks.json.loads((tasks.ROOT / tasks.DIRECTORY / 'tasks.json').read_text())
        self.assertEqual(len(specification['tasks']) * specification['repetitions'] * len(specification['setups']) * 3, 48)
        self.assertEqual(specification['limits']['wall_seconds'], 150)
        tools = {name: '/' + name for name in ('python', 'cargo', 'rustc', 'node', 'typescript')}
        self.assertEqual(tasks.public_checks('typescript-alias', tools)[0]['argv'][:2], ['/node', '/typescript'])
        with self.assertRaisesRegex(ValueError, 'explicit M2'):
            tasks.public_checks('unknown', tools)


if __name__ == '__main__':
    unittest.main()
