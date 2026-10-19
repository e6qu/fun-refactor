"""Reject changed source and rewritten records in completed cost studies."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('history', ROOT / 'tools/frozen-python-cost.py')
history = importlib.util.module_from_spec(spec)
spec.loader.exec_module(history)


class History(unittest.TestCase):
    def test_exact_clean_revision_is_required(self):
        for revision, status, accepted in (
            (history.SOURCE_COMMIT, '', True),
            ('0' * 40, '', False),
            (history.SOURCE_COMMIT, ' M src/project/flow_initializers.rs\n', False),
            (history.SOURCE_COMMIT, '?? tools/injected.py\n', False),
        ):
            with self.subTest(revision=revision, status=status), patch.object(
                history.subprocess, 'check_output', side_effect=[revision + '\n', status]
            ):
                if accepted:
                    history.check_source(Path('/separate-original-source'))
                else:
                    with self.assertRaisesRegex(ValueError, 'historical source .*changed'):
                        history.check_source(Path('/separate-original-source'))

    def test_current_checkout_cannot_be_used_as_history(self):
        with patch.object(history.subprocess, 'check_output', side_effect=[history.SOURCE_COMMIT, '']):
            with self.assertRaisesRegex(ValueError, 'separate source checkout'):
                history.check_source(ROOT)

    def test_published_reports_must_keep_their_original_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            source, published = Path(directory) / 'source', Path(directory) / 'published'
            source.mkdir()
            published.mkdir()
            for root in (source, published):
                (root / 'result.json').write_bytes(b'{"original":true}\n')
            history.check_published_reports(source, published, ('result.json',))
            (published / 'result.json').write_bytes(b'{"original":false}\n')
            with self.assertRaisesRegex(ValueError, 'historical report changed'):
                history.check_published_reports(source, published, ('result.json',))


if __name__ == '__main__':
    unittest.main()
