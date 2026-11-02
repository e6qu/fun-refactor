"""Algorithm grading must preserve both the program and every proof obligation."""
from pathlib import Path
import tempfile
import unittest
import hashlib
import json

from . import proof_algorithms as algorithms
from . import selected_tasks as common
from .selected_plan import ROOT, encoded


class AlgorithmChecks(unittest.TestCase):
    def setUp(self):
        self.text = (ROOT / algorithms.DIRECTORY / 'fixtures/Euclid.lean').read_text()

    def test_two_proof_regions_can_change_without_changing_program(self):
        changed = algorithms.insert(self.text, 'euclid_decreases', 'exact Nat.mod_lt a (Nat.pos_of_ne_zero hb)')
        changed = algorithms.insert(changed, 'euclid_correct', 'skip')
        self.assertEqual(algorithms.masked(self.text), algorithms.masked(changed))
        self.assertNotIn('sorry', changed)

    def test_reordered_duplicate_and_inline_markers_fail(self):
        for changed in (self.text.replace('euclid_decreases\n', 'euclid_correct\n', 1),
                        self.text + '-- fr:proof-begin unexpected\n',
                        self.text.replace('  -- fr:proof-end euclid_correct', '  skip -- fr:proof-end euclid_correct')):
            with self.subTest(text=changed), self.assertRaises(ValueError):
                algorithms.masked(changed)

    def test_modified_executable_claim_package_and_mode_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            path = project / algorithms.SPEC
            path.parent.mkdir(parents=True)
            path.write_text(self.text)
            package = project / 'specs/lakefile.toml'
            package.write_text('name = "frozen"\n')
            prepared = {'original': common.sources(project), 'protected_source': algorithms.masked(self.text)}
            for changed in (self.text.replace('then a else', 'then 0 else'),
                            self.text.replace('euclid a b = Nat.gcd a b', 'True'),
                            self.text + '\naxiom cheat : False\n'):
                path.write_text(changed)
                with self.assertRaises(ValueError):
                    algorithms.boundaries(project, prepared)
            path.write_text(self.text)
            package.write_text('name = "changed"\n')
            with self.assertRaises(ValueError):
                algorithms.boundaries(project, prepared)
            package.write_text('name = "frozen"\n')
            path.chmod(0o755)
            with self.assertRaises(ValueError):
                algorithms.boundaries(project, prepared)

    def test_axiom_audit_requires_function_and_both_proofs(self):
        text = "'Algorithm.euclid' does not depend on any axioms\n"
        with self.assertRaises(ValueError):
            algorithms.axiom_report(text)
        text += "'Algorithm.euclid_decreases' does not depend on any axioms\n"
        text += "'Algorithm.euclid_correct' depends on axioms: [propext, sorryAx]\n"
        self.assertEqual(algorithms.axiom_report(text)['euclid_correct'], ['propext', 'sorryAx'])
        self.assertFalse(set(algorithms.axiom_report(text)['euclid_correct']) <= common.APPROVED_AXIOMS)

    def test_seed_has_no_reference_proof(self):
        self.assertEqual(self.text.count('sorry'), 2)
        self.assertNotIn('Nat.strongRecOn', self.text)
        self.assertEqual(len(algorithms.spans(self.text)), 2)

    def test_entry_module_cannot_restore_the_automatic_large_import(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / algorithms.SPEC
            source.parent.mkdir(parents=True)
            source.write_text(self.text)
            entry = root / 'specs/FrSpecs.lean'
            entry.write_bytes((ROOT / algorithms.DIRECTORY / 'fixtures/FrSpecs.lean').read_bytes())
            prepared = {'original': common.sources(root), 'protected_source': algorithms.masked(self.text)}
            entry.write_text('import FrSpecs.Euclid\n')
            with self.assertRaisesRegex(ValueError, 'two declared proof regions'):
                algorithms.boundaries(root, prepared)

    def test_retained_agent_patch_keeps_exact_source_and_patch(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / 'patches').mkdir()
            patch = directory / 'patches/agent-task-1.patch'
            patch.write_text('original submitted patch\n')
            prepared = {'original': {'source': 'unchanged'}}
            value = {'task_source_sha256': hashlib.sha256(encoded(prepared['original'])).hexdigest(),
                     'attempts': [{'id': 'agent-task-1', 'patch_file': 'patches/agent-task-1.patch',
                                   'patch_sha256': common.digest(patch)}]}
            (directory / 'summary.json').write_text(json.dumps(value))
            self.assertEqual(algorithms.retained_submission(directory, prepared)[1], patch)
            with self.assertRaises(ValueError):
                algorithms.retained_submission(directory, {'original': {'source': 'different'}})
            patch.write_text('replacement patch\n')
            with self.assertRaises(ValueError):
                algorithms.retained_submission(directory, prepared)


if __name__ == '__main__':
    unittest.main()
