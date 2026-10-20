"""Count each user-facing message separately without splitting a long literal."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('prose', Path(__file__).with_name('check-prose.py'))
prose = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prose)


class MessageSentences(unittest.TestCase):
    def test_separate_short_messages_do_not_form_a_long_sentence(self):
        message = ' '.join(['word'] * 20)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'messages.rs'
            path.write_text(f'println!("{message}");\nbail!("{message}");\n')
            text = prose.messages_of(path)
        self.assertEqual(len(list(prose.sentences(text))), 2)
        self.assertEqual(prose.count(text)['long-sentence'], 0)

    def test_a_long_message_still_fails(self):
        message = ' '.join(['word'] * 26)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'messages.rs'
            path.write_text(f'bail!("{message}");\n')
            self.assertEqual(prose.count(prose.messages_of(path))['long-sentence'], 1)

    def test_a_continued_literal_remains_one_message(self):
        half = ' '.join(['word'] * 15)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'messages.rs'
            path.write_text('bail!("' + half + ' \\\n    ' + half + '");\n')
            text = prose.messages_of(path)
        self.assertEqual(len(list(prose.sentences(text))), 1)
        self.assertEqual(prose.count(text)['long-sentence'], 1)


if __name__ == '__main__':
    unittest.main()
