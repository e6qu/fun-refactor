#!/usr/bin/env python3
"""Reject unsafe snapshots, false no-path claims and premature held-out collection."""
import copy
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('repository_flow', ROOT / 'tools/python-repository-flow.py')
flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flow)


class RepositoryAdmission(unittest.TestCase):
    def archive(self, members):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode='w:gz') as archive:
            for name, kind in members:
                member = tarfile.TarInfo(name)
                member.type = kind
                data = b'license'
                if kind == tarfile.REGTYPE:
                    member.size = len(data)
                else:
                    member.linkname = '/tmp/outside'
                archive.addfile(member, io.BytesIO(data) if kind == tarfile.REGTYPE else None)
        return output.getvalue()

    def test_safe_exact_snapshot_has_a_license_and_inventory(self):
        project = {'repository': 'owner/example', 'revision': 'a' * 40}
        name = 'example-' + project['revision'] + '/LICENSE'
        with tempfile.TemporaryDirectory() as directory:
            inventory = flow.snapshot(self.archive([(name, tarfile.REGTYPE)]), project, Path(directory))
            self.assertEqual(inventory, {'LICENSE': flow.sha(b'license')})

    def test_links_traversal_duplicates_and_wrong_revisions_are_rejected(self):
        project = {'repository': 'owner/example', 'revision': 'a' * 40}
        prefix = 'example-' + project['revision'] + '/'
        for members in [[(prefix + '../LICENSE', tarfile.REGTYPE)],
                        [(prefix + 'LICENSE', tarfile.SYMTYPE)],
                        [(prefix + 'LICENSE', tarfile.LNKTYPE)],
                        [('other/LICENSE', tarfile.REGTYPE)],
                        [(prefix + 'LICENSE', tarfile.REGTYPE)] * 2]:
            with self.subTest(members=members), tempfile.TemporaryDirectory() as directory:
                with self.assertRaises(ValueError):
                    flow.snapshot(self.archive(members), project, Path(directory))

    def identity(self):
        run = {'id': 17, 'status': 'completed', 'conclusion': 'success', 'head_sha': 'a' * 40,
               'path': '.github/workflows/refinement-evidence.yml', 'html_url': 'https://example.invalid/17',
               'repository': {'full_name': 'owner/repository'}, 'head_repository': {'full_name': 'owner/repository'}}
        jobs = [{'name': 'coverage', 'run_id': 17, 'conclusion': 'success'}]
        return run, jobs

    def test_held_out_requires_success_on_the_same_frozen_revision(self):
        run, jobs = self.identity()
        self.assertEqual(flow.checked_coverage(run, jobs, 'a' * 40, 'owner/repository')['run_id'], 17)
        for field, value in [('head_sha', 'b' * 40), ('status', 'in_progress'),
                             ('conclusion', 'failure'), ('path', '.github/workflows/ci.yml')]:
            changed = {**run, field: value}
            with self.subTest(field=field), self.assertRaises(ValueError):
                flow.checked_coverage(changed, jobs, 'a' * 40, 'owner/repository')

    def test_forks_and_noncoverage_jobs_cannot_open_the_holdout(self):
        run, jobs = self.identity()
        for field in ('repository', 'head_repository'):
            changed = copy.deepcopy(run)
            changed[field]['full_name'] = 'other/fork'
            with self.assertRaises(ValueError):
                flow.checked_coverage(changed, jobs, 'a' * 40, 'owner/repository')
        for jobs in [[], [{'name': 'held-out', 'run_id': 17, 'conclusion': 'success'}],
                     [{'name': 'coverage', 'run_id': 99, 'conclusion': 'success'}]]:
            with self.assertRaises(ValueError):
                flow.checked_coverage(run, jobs, 'a' * 40, 'owner/repository')


class AnswerAuthority(unittest.TestCase):
    def row(self, case, complete=False, witnesses=None, cutoffs=None):
        return {'case': case, 'invocations': [{}, {'exit_code': 0, 'report': {
            'schema': 'fr-dataflow-1', 'claim': 'possible-value-propagation', 'complete': complete,
            'witnesses': witnesses or [], 'cutoffs': ['unresolved-local-import:external'] if cutoffs is None else cutoffs}}]}

    def test_incomplete_negative_answer_never_becomes_no_path(self):
        result = flow.assessment(self.row('overwritten'))
        self.assertEqual(result, {'status': 'incomplete', 'no_path': False})
        row = self.row('overwritten', complete=True, cutoffs=[])
        self.assertEqual(flow.assessment(row), {'status': 'complete', 'no_path': True})

    def test_unknowns_and_exhausted_budgets_cannot_claim_completeness(self):
        for case in ('unknown', 'steps'):
            with self.subTest(case=case), self.assertRaises(ValueError):
                flow.assessment(self.row(case, complete=True, cutoffs=[]))

    def test_missing_boundary_or_wrong_authority_is_rejected(self):
        row = self.row('overwritten', cutoffs=[])
        with self.assertRaises(ValueError):
            flow.assessment(row)
        row = self.row('overwritten')
        row['invocations'][1]['report']['claim'] = 'proven-no-path'
        with self.assertRaises(ValueError):
            flow.assessment(row)

    def test_complete_answers_must_agree_with_independent_source_inspection(self):
        for row in (self.row('positive', complete=True, cutoffs=[]),
                    self.row('overwritten', complete=True, witnesses=[{}], cutoffs=[])):
            with self.assertRaises(ValueError):
                flow.assessment(row)


if __name__ == '__main__':
    unittest.main()
