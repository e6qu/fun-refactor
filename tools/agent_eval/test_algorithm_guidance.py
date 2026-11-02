"""Keep the experimental comparison fair and every scheduled slot visible."""
import json
from pathlib import Path
import tempfile
import unittest

from . import algorithm_guidance as guidance
from . import selected_local as local
from .selected_plan import ROOT, prepare


class AlgorithmGuidanceChecks(unittest.TestCase):
    def test_eight_fact_slots_and_twelve_presentation_slots_reverse_order(self):
        for study, count in [('M3-algorithm-facts', 8), ('M3-algorithm-presentation', 12)]:
            plan = prepare(ROOT, study)
            self.assertEqual(len(plan['attempts']), count)
            self.assertEqual(plan['requirements']['limits'], {**local.LIMITS, 'parallel_attempts': 1})
            self.assertEqual(plan['requirements']['client_finish_seconds'], 10)
            for profile in plan['profiles']:
                rows = [row for row in plan['attempts'] if row['profile'] == profile]
                self.assertEqual([row['setup'] for row in rows[:2]], [row['setup'] for row in rows[2:]][::-1])
                self.assertEqual({row['task'] for row in rows}, {'euclid'})
            for path in ('requirements.json', 'guidance.json', 'declarations.txt', 'fixtures/Euclid.lean',
                         'controls/euclid-correct.lean', 'controls/euclid-decreases.lean'):
                self.assertIn(str(guidance.DIRECTORY / path), plan['bindings'])

    def test_presentations_have_exactly_the_same_nodes_edges_and_explanation(self):
        plan = json.loads((ROOT / guidance.DIRECTORY / 'guidance.json').read_text())
        checklist, graph = [guidance.render(plan, condition) for condition in ('checklist', 'graph')]
        normalized_checklist = '\n'.join(line.removeprefix('- ') for line in checklist.splitlines() if line)
        normalized_graph = '\n'.join(line.replace(' -- ', ' ').replace(' --> ', ' ') for line in graph.splitlines() if line)
        self.assertEqual(normalized_checklist, normalized_graph)
        for text in (checklist, graph):
            for node in plan['nodes']:
                self.assertEqual(text.count(node['text']), 1)
            self.assertEqual(text.count(plan['meaning']), 1)
        self.assertNotEqual(checklist, graph)

    def test_fact_conditions_change_only_declaration_handout(self):
        with tempfile.TemporaryDirectory() as temporary:
            contents = []
            for condition in ('select-facts', 'supplied-declarations'):
                project = Path(temporary) / condition
                project.mkdir()
                protected = guidance.install(project, condition, 'M3-algorithm-facts')
                self.assertEqual(local.checking_input_changes(project, protected), [])
                contents.append({name: (project / name).read_text() for name in protected})
                (project / '.fr/proof-plan.txt').write_text('changed')
                self.assertEqual(local.checking_input_changes(project, protected), ['.fr/proof-plan.txt'])
                with self.assertRaises(ValueError):
                    guidance.install(project, condition, 'M3-algorithm-facts')
            self.assertEqual(contents[0]['.fr/proof-plan.txt'], contents[1]['.fr/proof-plan.txt'])
            self.assertNotEqual(contents[0]['.fr/declarations.txt'], contents[1]['.fr/declarations.txt'])

    def test_every_condition_gets_fr_and_complete_execution_instructions(self):
        checkers = {'lean': {'path': '/pinned/lean', 'source_root': '/pinned/src'}, 'lake': {'path': '/pinned/lake'}}
        for study in guidance.STUDIES:
            prompts = [local.prompt('euclid', condition, checkers, study)
                       for condition in prepare(ROOT, study)['requirements']['setups']]
            self.assertEqual(*prompts)
            for phrase in ('Fill BOTH', 'Fr and its project skill are installed', '150-second deadline',
                           'Do not start a nested guard', 'Preserve provided .fr/checks.json', 'change.patch'):
                self.assertIn(phrase, prompts[0])
            self.assertNotIn('Fr is not provided', prompts[0])


if __name__ == '__main__':
    unittest.main()
