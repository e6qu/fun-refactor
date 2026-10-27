"""The source review must preserve duplicates and distinguish changed resolution."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('reader', Path(__file__).with_name('lean-reader-compare.py'))
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)


class ReaderComparison(unittest.TestCase):
    def packet(self):
        symbol = {'identity': {'file': 'Proof.lean', 'name': 'proof', 'kind': 'function',
                              'name_span': {'start': 8, 'end': 13}}, 'full_span': {'start': 0, 'end': 50}}
        reference = {'file': 'Proof.lean', 'span': {'start': 30, 'end': 35}, 'name': 'proof',
                     'kind': 'read', 'twin': False, 'target': None, 'confidence': 'name-only'}
        return {'sources': {'Proof.lean': 'a' * 64}, 'symbols': [symbol], 'references': [reference]}

    def test_changed_target_is_not_a_removed_occurrence(self):
        before = self.packet()
        after = copy.deepcopy(before)
        after['references'][0].update(target=before['symbols'][0]['identity'], confidence='exact')
        result = reader.compare(before, after)
        self.assertEqual(result['removed_references'], [])
        self.assertEqual(result['added_references'], [])
        self.assertEqual(len(result['changed_references']), 1)

    def test_duplicate_reference_and_symbol_occurrences_are_preserved(self):
        before = self.packet()
        before['references'] *= 2
        before['symbols'] *= 2
        after = self.packet()
        result = reader.compare(before, after)
        self.assertEqual(len(result['removed_references']), 1)
        self.assertEqual(len(result['removed_symbols']), 1)
        self.assertEqual(result['changed_references'], [])
        self.assertEqual(result['changed_symbols'], [])

    def test_changed_source_and_changed_site_cannot_masquerade_as_resolution(self):
        before = self.packet()
        after = copy.deepcopy(before)
        after['sources']['Proof.lean'] = 'b' * 64
        with self.assertRaises(AssertionError):
            reader.compare(before, after)
        after['sources'] = before['sources']
        after['references'][0]['span'] = {'start': 36, 'end': 41}
        result = reader.compare(before, after)
        self.assertEqual(len(result['removed_references']), 1)
        self.assertEqual(len(result['added_references']), 1)
        self.assertEqual(result['changed_references'], [])


if __name__ == '__main__':
    unittest.main()
