"""Exercise missing downloads, altered receipts and mixed release revisions."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

import product_bundle as bundle


class ProductBundle(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.version = '1.2.3'
        self.source = 'a' * 40
        self.checker = 'b' * 40
        self.identity = {'source_commit': self.source, 'checker_commit': self.checker}
        self.consumer = {'passed': True, 'version': self.version, 'distribution_version': None,
                         'checks': ['graph-and-source-digests', 'read-only-preview', 'rename-and-behavior',
                                    'fresh-graph-after-edit', 'undo-redo', 'fresh-receiver-patch', 'stale-preview-refusal']}
        for target in ['x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl',
                       'x86_64-apple-darwin', 'aarch64-apple-darwin']:
            self.archive(f'fr-v1.2.3-{target}', 'native', self.consumer)
        self.archive('fun-refactor-v1.2.3-wasm', 'wasm', {'passed': True, 'checks': ['api', 'review', 'patch', 'session']})
        sdk_consumer = {**self.consumer, 'distribution_version': self.version}
        receipt = {'schema': 'fr-sdk-package-check-2', 'passed': True, 'version': self.version,
                   'provenance': self.identity}
        names = {'wheel': 'fun_refactor_ir-1.2.3-py3-none-any.whl', 'sdist': 'fun_refactor_ir-1.2.3.tar.gz'}
        for kind, name in names.items():
            (self.root / name).write_bytes(('opaque distribution: ' + kind).encode())
            receipt[kind] = {'name': name, 'sha256': bundle.digest(self.root / name), 'consumer': sdk_consumer}
        self.write_json('fun-refactor-ir-v1.2.3.check.json', receipt)
        (self.root / 'fun-refactor-ir-v1.2.3.sha256').write_text(''.join(
            bundle.digest(self.root / name) + '  ' + name + '\n' for name in names.values()))
        self.native = 'fr-v1.2.3-x86_64-unknown-linux-musl.check.json'
        self.sdk = 'fun-refactor-ir-v1.2.3.check.json'

    def write_json(self, name, value):
        (self.root / name).write_text(json.dumps(value))

    def archive(self, prefix, kind, consumer):
        name = prefix + '.tar.gz'
        (self.root / name).write_bytes(('opaque archive: ' + prefix).encode())
        digest = bundle.digest(self.root / name)
        (self.root / (name + '.sha256')).write_text(digest + '  ' + name + '\n')
        self.write_json(prefix + '.check.json', {'schema': 'fr-release-package-check-1',
            'version': self.version, 'provenance': self.identity, 'kind': kind, 'archive': name,
            'archive_sha256': digest, 'consumer': consumer})

    def check(self, **kwargs):
        return bundle.qualify(self.root, self.version, self.source, self.checker, **kwargs)

    def test_complete_set_is_seven_downloads_nineteen_bound_files(self):
        result = self.check(write=True)
        self.assertEqual(len(result['packages']), 7)
        self.assertEqual(len(result['files']), 19)
        self.assertEqual(result['provenance'], self.identity)
        self.assertEqual(self.check(), result)
        self.assertEqual(len(list(self.root.iterdir())), 20)

    def test_every_payload_checksum_and_receipt_is_required(self):
        for path in list(self.root.iterdir()):
            saved = path.read_bytes()
            path.unlink()
            with self.subTest(name=path.name), self.assertRaisesRegex(ValueError, 'file set'):
                self.check(write=True)
            path.write_bytes(saved)

    def test_extra_files_directories_and_links_are_rejected(self):
        path = self.root / 'unexpected'
        path.write_text('extra')
        with self.assertRaisesRegex(ValueError, 'file set'):
            self.check(write=True)
        path.unlink()
        path.mkdir()
        with self.assertRaisesRegex(ValueError, 'non-file'):
            self.check(write=True)
        path.rmdir()
        path.symlink_to(self.root / self.native)
        with self.assertRaisesRegex(ValueError, 'link'):
            self.check(write=True)

    def test_versions_sources_and_checker_revisions_cannot_be_mixed(self):
        original = json.loads((self.root / self.native).read_text())
        for field, value in [('version', '1.2.2'), ('provenance', None),
                             ('provenance', {**self.identity, 'source_commit': 'c' * 40}),
                             ('provenance', {**self.identity, 'checker_commit': 'd' * 40})]:
            self.write_json(self.native, {**original, field: value})
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                self.check(write=True)
        self.write_json(self.native, original)
        with self.assertRaisesRegex(ValueError, 'file set'):
            bundle.qualify(self.root, '1.2.2', self.source, self.checker, write=True)

    def test_receipt_cannot_be_relabelled_for_another_archive(self):
        original = json.loads((self.root / self.native).read_text())
        for field, value in [('archive', 'fr-v1.2.3-aarch64-unknown-linux-musl.tar.gz'),
                             ('kind', 'wasm'), ('archive_sha256', '0' * 64)]:
            self.write_json(self.native, {**original, field: value})
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.check(write=True)

    def test_a_changed_archive_cannot_pass_with_only_a_new_sidecar(self):
        archive = self.root / 'fr-v1.2.3-x86_64-unknown-linux-musl.tar.gz'
        archive.write_bytes(b'changed artifact')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            self.check(write=True)
        (self.root / (archive.name + '.sha256')).write_text(bundle.digest(archive) + '  ' + archive.name + '\n')
        with self.assertRaisesRegex(ValueError, 'receipt digest'):
            self.check(write=True)

    def test_failed_missing_and_duplicate_consumer_checks_refuse(self):
        original = json.loads((self.root / self.native).read_text())
        for change in [{'passed': False}, {'passed': 1}, {'checks': []},
                       {'checks': self.consumer['checks'] * 2}, {'version': '1.2.2'}]:
            self.write_json(self.native, {**original, 'consumer': {**self.consumer, **change}})
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.check(write=True)

    def test_browser_checks_are_required(self):
        name = 'fun-refactor-v1.2.3-wasm.check.json'
        value = json.loads((self.root / name).read_text())
        value['consumer']['checks'].remove('patch')
        self.write_json(name, value)
        with self.assertRaisesRegex(ValueError, 'consumer checks'):
            self.check(write=True)

    def test_both_installed_sdk_consumers_must_pass_and_match(self):
        original = json.loads((self.root / self.sdk).read_text())
        for kind in ['wheel', 'sdist']:
            for change in [{'passed': False}, {'distribution_version': '1.2.2'}, {'checks': []}]:
                value = copy.deepcopy(original)
                value[kind]['consumer'].update(change)
                self.write_json(self.sdk, value)
                with self.subTest(kind=kind, change=change), self.assertRaises(ValueError):
                    self.check(write=True)

    def test_duplicate_json_keys_and_checksum_entries_refuse(self):
        path = self.root / self.native
        original = path.read_text()
        path.write_text(original[:-1] + ', "version": "1.2.3"}')
        with self.assertRaisesRegex(ValueError, 'duplicate JSON'):
            self.check(write=True)
        path.write_text(original)
        checksum = self.root / 'fun-refactor-ir-v1.2.3.sha256'
        checksum.write_text(checksum.read_text() * 2)
        with self.assertRaisesRegex(ValueError, 'duplicate checksum'):
            self.check(write=True)

    def test_manifest_cannot_be_overwritten_or_relabelled(self):
        self.check(write=True)
        with self.assertRaisesRegex(ValueError, 'file set'):
            self.check(write=True)
        path = self.root / bundle.manifest_name(self.version)
        value = json.loads(path.read_text())
        value['provenance']['source_commit'] = 'c' * 40
        self.write_json(path.name, value)
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            self.check()

    def test_receipt_changes_after_qualification_invalidate_the_manifest(self):
        self.check(write=True)
        value = json.loads((self.root / self.native).read_text())
        value['additional_note'] = 'altered after qualification'
        self.write_json(self.native, value)
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            self.check()

    def test_identity_requires_two_full_commits(self):
        self.assertIsNone(bundle.provenance(None, None))
        for source, checker in [(None, self.checker), (self.source, None), ('main', self.checker),
                                (self.source, '../escape'), ('a' * 39, self.checker)]:
            with self.subTest(source=source, checker=checker), self.assertRaisesRegex(ValueError, 'Git object IDs'):
                bundle.provenance(source, checker)
        for version in ['../outside', '1.2.3\nextra', '1.2', '1.2.3-rc1']:
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, 'release version'):
                bundle.manifest_name(version)


if __name__ == '__main__':
    unittest.main()
