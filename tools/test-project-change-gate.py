"""Keep failures on every page of the project-change run history."""
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

import project_change_gate as gate


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


if __name__ == '__main__':
    unittest.main()
