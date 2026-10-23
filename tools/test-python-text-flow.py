import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('text_flow', Path(__file__).with_name('python-text-flow.py'))
text_flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(text_flow)


class TextFlow(unittest.TestCase):
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
