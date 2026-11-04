import copy
import importlib.util
import json
from pathlib import Path
import unittest
import tempfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('text_flow', Path(__file__).with_name('python-text-flow.py'))
text_flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(text_flow)


class TextFlow(unittest.TestCase):
    def test_release_metadata_keeps_inputs_but_sdk_code_changes_invalidate_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('tools/python-text-flow.py', 'tools/test-python-text-flow.py', 'tools/evidence_basis.py',
                         'tests/language_behavior.rs', 'sdk/python/tests/test_text_flow.py',
                         'docs/check-language-behavior.md', 'tests/agent-eval/python-text-flow/cases.json',
                         'sdk/python/src/fr_ir/logic.py'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('fixed input\n')
            manifest = root / 'Cargo.toml'
            version = root / 'sdk/python/src/fr_ir/_version.py'
            def release(number):
                manifest.write_text('[package]\nversion = "' + number + '"\n')
                version.write_text('VERSION = "' + number + '"  # x-release-please-version\n')
            release('1.0.0')
            with patch.object(text_flow, 'ROOT', root), \
                 patch.object(text_flow, 'FIXTURE', root / 'tests/agent-eval/python-text-flow'), \
                 patch.object(text_flow, 'native_source_bindings', return_value={'native': 'fixed'}):
                baseline = text_flow.bindings()
                release('2.0.0')
                self.assertEqual(text_flow.bindings(), baseline)
                (root / 'sdk/python/src/fr_ir/logic.py').write_text('changed behavior\n')
                self.assertNotEqual(text_flow.bindings(), baseline)
                version.write_text('VERSION = "3.0.0"  # x-release-please-version\n')
                with self.assertRaisesRegex(ValueError, 'versions must match'):
                    text_flow.bindings()

    def test_conversion_keeps_customer_input_and_requires_independent_python_results(self):
        cases = json.loads((text_flow.FIXTURE / 'cases.json').read_text())
        case, = [row for row in cases if row['id'] == 'convert-lower']
        report = {'complete': True, 'witnesses': [{}],
            'runtime_effects': {'exception_safety_established': False}}
        observed = {key: case[key] for key in ('sent', 'returned', 'raised')}
        text_flow.validate(case, report, observed)
        for change in ('lost-input', 'runtime-result', 'exception-claim'):
            altered, execution = copy.deepcopy(report), copy.deepcopy(observed)
            if change == 'lost-input':
                altered['witnesses'] = []
            elif change == 'runtime-result':
                execution['returned'] = 'different'
            else:
                altered['runtime_effects']['exception_safety_established'] = True
            with self.assertRaises(ValueError):
                text_flow.validate(case, altered, execution)

    def test_unknown_receivers_cannot_be_reported_as_a_complete_absence(self):
        cases = json.loads((text_flow.FIXTURE / 'cases.json').read_text())
        case, = [row for row in cases if row['id'] == 'object-reassignment']
        report = {'complete': False, 'witnesses': [],
            'runtime_effects': {'exception_safety_established': False}}
        observed = {key: case[key] for key in ('sent', 'returned', 'raised')}
        text_flow.validate(case, report, observed)
        report['complete'] = True
        with self.assertRaisesRegex(ValueError, 'completion'):
            text_flow.validate(case, report, observed)


if __name__ == '__main__':
    unittest.main()
