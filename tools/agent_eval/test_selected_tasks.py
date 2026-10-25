"""Protect the fixed task and independent check boundaries from candidate edits."""
from pathlib import Path
import hashlib
import json
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_tasks as tasks


class SelectedTaskTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_renamed_consumers_remain_in_the_original_scope(self):
        caller = self.root / 'consumer.py'
        caller.write_text('from boltons.mathutils import clamp\nclamp(70, 0, 50)\n')
        prepared = {'original': tasks.sources(self.root), 'rename_paths': ['consumer.py']}
        caller.write_text('from boltons.mathutils import clamp_value\nclamp_value(70, 0, 50)\n')
        self.assertEqual(tasks.unchanged_boundaries('boltons-rename', self.root, prepared), ['consumer.py'])
        unrelated = self.root / tasks.UNRELATED
        unrelated.write_text('def clamp(value): return value\n')
        with self.assertRaisesRegex(ValueError, 'unrelated clamp'):
            tasks.unchanged_boundaries('boltons-rename', self.root, prepared)

    def test_candidate_cannot_expand_rename_scope_by_adding_the_old_name(self):
        unrelated = self.root / 'settings.py'
        unrelated.write_text('enabled = False\n')
        prepared = {'original': tasks.sources(self.root), 'rename_paths': []}
        unrelated.write_text('enabled = True  # clamp\n')
        with self.assertRaisesRegex(ValueError, 'outside its declared consumers'):
            tasks.unchanged_boundaries('boltons-rename', self.root, prepared)

    def test_proof_may_change_steps_but_not_the_fixed_property(self):
        path = self.root / tasks.PROOF_PATH
        path.parent.mkdir(parents=True)
        source = ('theorem allowedModel_access_rule (staff owner : Bool) :\n'
                  '    allowedModel staff owner = (staff || owner) := by\n'
                  '-- fr:proof-begin allowedModel_access_rule\n'
                  '  sorry\n-- fr:proof-end allowedModel_access_rule\n')
        path.write_text(source)
        prepared = {'original': tasks.sources(self.root), 'protected_proof': tasks.masked_proof(source)}
        path.write_text(source.replace('sorry', 'rfl'))
        self.assertEqual(tasks.unchanged_boundaries('access-proof', self.root, prepared), [tasks.PROOF_PATH])
        path.write_text(source.replace('staff || owner', 'staff && owner'))
        with self.assertRaisesRegex(ValueError, 'definitions, claim, assumptions'):
            tasks.unchanged_boundaries('access-proof', self.root, prepared)

    def test_deleted_proof_or_source_symlink_cannot_hide_a_changed_definition(self):
        path = self.root / tasks.PROOF_PATH
        path.parent.mkdir(parents=True)
        path.write_text('protected source')
        prepared = {'original': tasks.sources(self.root)}
        path.unlink()
        with self.assertRaisesRegex(ValueError, 'proof file is missing'):
            tasks.unchanged_boundaries('access-proof', self.root, prepared)
        (self.root / 'hidden-source').symlink_to(path.parent, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'must not be symlinks'):
            tasks.sources(self.root)

    def test_retained_seed_contains_only_the_exact_unfinished_source(self):
        project, output = self.root / 'project', self.root / 'evidence'
        project.mkdir()
        output.mkdir()
        (project / 'claim.lean').write_text('theorem unfinished : True := by sorry\n')
        (project / '.lake').mkdir()
        (project / '.lake/private-cache').write_text('not part of the task')
        prepared = {'task': 'access-proof', 'original': tasks.sources(project)}
        receipt = tasks.retain_seed(project, prepared, output)
        with tarfile.open(output / 'original.tar.gz') as archive:
            self.assertEqual(archive.getnames(), ['claim.lean'])
            self.assertIn(b'sorry', archive.extractfile('claim.lean').read())
        self.assertEqual(receipt['archive_sha256'], hashlib.sha256((output / 'original.tar.gz').read_bytes()).hexdigest())
        self.assertEqual(json.loads((output / 'seed.json').read_text()), receipt)
        (project / 'claim.lean').write_text('theorem unfinished : True := by trivial\n')
        with self.assertRaisesRegex(ValueError, 'differs from the prepared original'):
            tasks.retain_seed(project, prepared, output)


if __name__ == '__main__':
    unittest.main()
