"""Keep failures on every page of the project-change run history."""
from types import SimpleNamespace
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

import project_change_gate as gate

spec = importlib.util.spec_from_file_location('project_changes', Path(__file__).with_name('project-changes.py'))
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class AttemptHistory(unittest.TestCase):
    def test_reads_older_failures_beyond_the_first_page(self):
        runs = [{'id': index, 'conclusion': 'failure' if index == 1 else 'success'}
                for index in range(101, 0, -1)]
        pages = []
        cutoff = '2026-10-20T12:00:00Z'

        def api(route):
            if route.endswith('/actions/runs/101'):
                return {'created_at': cutoff}
            if '/workflows/' in route:
                query = parse_qs(urlsplit(route).query)
                self.assertEqual(query['created'], ['<=' + cutoff])
                self.assertEqual(query['branch'], ['feature/project-changes'])
                page = int(query['page'][0])
                pages.append(page)
                return {'total_count': len(runs), 'workflow_runs': runs[(page - 1) * 100:page * 100]}
            run_id = int(route.split('/runs/')[1].split('/')[0])
            return {'jobs': [{'name': 'project-changes', 'conclusion': 'failure' if run_id == 1 else 'skipped'}]}

        with patch.object(gate, 'retained_tools', return_value=SimpleNamespace(retain=SimpleNamespace(api=api))):
            result = gate.attempts('owner/repository', 'feature/project-changes', 101)
        self.assertEqual(pages, [1, 2])
        self.assertEqual([row['run']['id'] for row in result], [1])
        self.assertEqual(result[0]['run']['conclusion'], 'failure')


class InspectionQualification(unittest.TestCase):
    def test_only_a_clean_inspection_can_report_success(self):
        for changes in ('', ' M tests/host_recovery.rs\n', '?? tools/new-rule.py\n'):
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as temporary:
                args = SimpleNamespace(output=Path(temporary) / 'inspection', fr=Path(temporary) / 'fr')
                report = SimpleNamespace(to_data=lambda: {'page': {'next': None}, 'rows': [['function']]})
                client = SimpleNamespace(project=lambda *arguments: report)

                def git(command, **settings):
                    return changes if 'status' in command else 'a' * 40

                with patch.object(collector, 'FrClient', return_value=client), \
                        patch.object(collector, 'bindings', return_value={}), \
                        patch.object(collector.subprocess, 'check_output', side_effect=git):
                    if changes:
                        with self.assertRaisesRegex(ValueError, 'Commit the runner corrections'):
                            collector.inspect(args)
                    else:
                        collector.inspect(args)
                result = json.loads((args.output / 'scope.json').read_text())
                self.assertIs(result['passed'], not changes)
                self.assertIs(result['source_clean'], not changes)
                self.assertEqual(result['changed_inputs'], changes.splitlines())


if __name__ == '__main__':
    unittest.main()
