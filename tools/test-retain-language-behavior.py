#!/usr/bin/env python3
"""Reject unsafe and missing language files before any evidence write."""
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('language_retention', Path(__file__).with_name('retain-language-behavior.py'))
retention = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retention)


class Inventory(unittest.TestCase):
    def test_only_the_exact_named_result_files_are_selected(self):
        prefix = 'language-behavior/translation/'
        data = {prefix + 'result.json': json.dumps({'translations': [{'files': {'source.rs': 'digest'}}]}).encode(),
                prefix + 'source.rs': b'fn source() {}', prefix + 'unused.bin': b'not retained'}
        self.assertEqual(set(retention.selected_files(data, 'checked-translation')), {'result.json', 'source.rs'})

    def test_unsafe_missing_and_repeated_names_refuse_before_writing(self):
        prefix = 'language-behavior/translation/'
        for rows in ([{'files': {'../escape': 'digest'}}], [{'files': {'/escape': 'digest'}}],
                     [{'files': {'missing.rs': 'digest'}}], [{'files': {'result.json': 'digest'}}]):
            data = {prefix + 'result.json': json.dumps({'translations': rows}).encode()}
            with self.subTest(rows=rows), self.assertRaises((ValueError, KeyError)):
                retention.selected_files(data, 'checked-translation')


if __name__ == '__main__':
    unittest.main()
