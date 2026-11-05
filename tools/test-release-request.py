"""No release calls: validate published-tag admission and immutable source selection."""
import base64
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import Mock, call

spec = importlib.util.spec_from_file_location("release_request", Path(__file__).with_name("release-request.py"))
release_request = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_request)


class ReleaseRequest(unittest.TestCase):
    def setUp(self):
        self.tag, self.commit = "fun-refactor-v0.80.1", "a" * 40
        self.release = {"tag_name": self.tag, "id": 42, "draft": False, "prerelease": False,
                        "published_at": "2026-10-22T00:00:00Z", "assets": []}
        self.manifest = '[package]\nname="fun-refactor"\nversion="0.80.1"\n'
        self.fetch = Mock(side_effect=self.response)

    def response(self, repository, path):
        self.assertEqual(repository, "e6qu/fun-refactor")
        return {f"releases/tags/{self.tag}": self.release, "releases/latest": self.release,
                f"commits/{self.tag}": {"sha": self.commit},
                f"contents/Cargo.toml?ref={self.commit}": {"path": "Cargo.toml", "encoding": "base64",
                    "content": base64.b64encode(self.manifest.encode()).decode()}}[path]

    def manual(self):
        return release_request.select("workflow_dispatch", self.tag, "", "", "", "e6qu/fun-refactor", self.fetch)

    def test_existing_empty_release_uses_tag_commit_instead_of_workflow_branch(self):
        self.assertEqual(self.manual(), {"released": "true", "tag": self.tag, "version": "0.80.1", "commit": self.commit})
        self.assertIn(call("e6qu/fun-refactor", f"contents/Cargo.toml?ref={self.commit}"), self.fetch.call_args_list)

    def test_ordinary_push_without_release_does_not_query_or_build(self):
        for created in ("", "false"):
            result = release_request.select("push", "", created, "", "", "e6qu/fun-refactor", self.fetch)
            self.assertEqual(result["released"], "false")
        self.fetch.assert_not_called()

    def test_automatic_creation_selects_the_same_immutable_tag(self):
        result = release_request.select("push", "", "true", self.tag, "0.80.1", "e6qu/fun-refactor", self.fetch)
        self.assertEqual(result["commit"], self.commit)
        self.assertNotIn(call("e6qu/fun-refactor", "releases/latest"), self.fetch.call_args_list)

    def test_existing_downloads_and_unpublished_releases_are_rejected(self):
        original = copy.deepcopy(self.release)
        for field, value in (("assets", [{"name": "existing.tar.gz"}]), ("draft", True),
                             ("prerelease", True), ("published_at", None)):
            self.release = {**original, field: value}
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.manual()

    def test_older_release_is_rejected(self):
        self.fetch.side_effect = lambda repository, path: {"id": 99} if path == "releases/latest" else self.response(repository, path)
        with self.assertRaisesRegex(ValueError, "latest"):
            self.manual()

    def test_source_version_mismatch_does_not_relabel_a_release(self):
        self.manifest = self.manifest.replace("0.80.1", "0.80.2")
        with self.assertRaisesRegex(ValueError, "package version"):
            self.manual()

    def test_tag_or_api_failure_never_uses_main_as_a_substitute(self):
        self.fetch.side_effect = RuntimeError("GitHub query failed")
        with self.assertRaisesRegex(RuntimeError, "GitHub query failed"):
            self.manual()
        for tag in ("main", "", "fun-refactor-v0.80.1\ncommit=main", "../v0.80.1"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release_request.select("workflow_dispatch", tag, "", "", "", "e6qu/fun-refactor", self.fetch)

    def test_planned_release_collects_a_real_replay_before_requesting_full_ci(self):
        planned = {'number': 42, 'baseBranchName': 'main',
                   'headBranchName': 'release-please--branches--main'}
        pull = {'state': 'open', 'draft': False, 'base': {'ref': 'main'},
                'head': {'ref': planned['headBranchName'], 'repo': {'full_name': 'e6qu/fun-refactor'}}}
        fetch = Mock(return_value=pull)
        self.assertEqual(release_request.replay_request(planned, 'e6qu/fun-refactor', '123', fetch),
            ['gh', 'workflow', 'run', 'project-changes-recheck.yml', '--repo', 'e6qu/fun-refactor',
             '--ref', planned['headBranchName'], '-f', 'prefix=release-123', '-f', 'run_ci=true'])
        fetch.assert_called_once_with('e6qu/fun-refactor', 'pulls/42')

    def test_planning_cannot_refresh_main_a_fork_or_an_unrelated_pull_request(self):
        planned = {'number': 42, 'baseBranchName': 'main',
                   'headBranchName': 'release-please--branches--main'}
        pull = {'state': 'open', 'draft': False, 'base': {'ref': 'main'},
                'head': {'ref': planned['headBranchName'], 'repo': {'full_name': 'e6qu/fun-refactor'}}}
        for change in ({'number': True}, {'number': -1}, {'baseBranchName': 'other'},
                       {'headBranchName': 'main'}, {'headBranchName': 'release-please--branches--main\nother'}):
            fetch = Mock(return_value=pull)
            with self.subTest(planned=change), self.assertRaises(ValueError):
                release_request.replay_request({**planned, **change}, 'e6qu/fun-refactor', '123', fetch)
            fetch.assert_not_called()
        for change in ({'state': 'closed'}, {'draft': True}, {'base': {'ref': 'other'}},
                       {'head': {'ref': 'main', 'repo': {'full_name': 'e6qu/fun-refactor'}}},
                       {'head': {'ref': planned['headBranchName'], 'repo': {'full_name': 'other/fun-refactor'}}}):
            with self.subTest(pull=change), self.assertRaises(ValueError):
                release_request.replay_request(planned, 'e6qu/fun-refactor', '123', Mock(return_value={**pull, **change}))

    def test_planning_api_errors_and_invalid_run_ids_stop_dispatch(self):
        planned = {'number': 42, 'baseBranchName': 'main',
                   'headBranchName': 'release-please--branches--main'}
        for run in ('', '0', '-1', '123\nmain', '1' * 21):
            fetch = Mock()
            with self.subTest(run=run), self.assertRaises(ValueError):
                release_request.replay_request(planned, 'e6qu/fun-refactor', run, fetch)
            fetch.assert_not_called()
        with self.assertRaisesRegex(RuntimeError, 'GitHub query failed'):
            release_request.replay_request(planned, 'e6qu/fun-refactor', '123',
                                          Mock(side_effect=RuntimeError('GitHub query failed')))


if __name__ == "__main__":
    unittest.main()
