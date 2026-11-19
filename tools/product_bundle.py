"""Verify the complete release download set and its recorded consumer checks."""
import hashlib
import json
from pathlib import Path
import re

TARGETS = ('x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl',
           'x86_64-apple-darwin', 'aarch64-apple-darwin')
CLI_CHECKS = {'graph-and-source-digests', 'read-only-preview', 'rename-and-behavior',
              'fresh-graph-after-edit', 'undo-redo', 'fresh-receiver-patch', 'stale-preview-refusal'}
WASM_CHECKS = {'api', 'review', 'patch', 'session'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def provenance(source_commit, checker_commit):
    if source_commit is None and checker_commit is None:
        return None
    for value in (source_commit, checker_commit):
        require(isinstance(value, str) and re.fullmatch(r'[0-9a-f]{40}', value),
                'source and checker commits must both be full Git object IDs')
    return {'source_commit': source_commit, 'checker_commit': checker_commit}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key: ' + key)
        result[key] = value
    return result


def read_json(path):
    require(path.stat().st_size <= 1024 * 1024, 'receipt or manifest exceeds 1 MiB')
    value = json.loads(path.read_text(), object_pairs_hook=unique_object)
    require(isinstance(value, dict), 'receipt or manifest must be a JSON object')
    return value


def consumer(report, version, required, *, sdk=False, wasm=False):
    require(isinstance(report, dict) and report.get('passed') is True, 'consumer did not pass')
    checks = report.get('checks')
    require(isinstance(checks, list) and all(isinstance(item, str) for item in checks), 'invalid consumer checks')
    require(len(checks) == len(set(checks)) and required <= set(checks), 'missing or duplicate consumer checks')
    if not wasm:
        require(report.get('version') == version, 'consumer version differs from release')
        require(report.get('distribution_version') == (version if sdk else None), 'installed SDK version differs from release')
    return sorted(checks)


def manifest_name(version):
    require(isinstance(version, str) and re.fullmatch(r'\d+\.\d+\.\d+', version),
            'bundle requires a stable three-part release version')
    return 'fun-refactor-v' + version + '.qualification.json'


def inspect(directory, version, source_commit, checker_commit, *, with_manifest=False):
    identity = provenance(source_commit, checker_commit)
    require(identity is not None, 'bundle requires source and checker commits')
    name = manifest_name(version)
    archives = [(f'fr-v{version}-{target}', 'native', target) for target in TARGETS]
    archives.append((f'fun-refactor-v{version}-wasm', 'wasm', 'wasm32-unknown-unknown'))
    sdk_prefix = f'fun-refactor-ir-v{version}'
    wheel = f'fun_refactor_ir-{version}-py3-none-any.whl'
    sdist = f'fun_refactor_ir-{version}.tar.gz'
    sdk_receipt = sdk_prefix + '.check.json'
    checksum_files = {prefix + '.tar.gz.sha256': {prefix + '.tar.gz'} for prefix, _, _ in archives}
    checksum_files[sdk_prefix + '.sha256'] = {wheel, sdist}
    receipts = {prefix + '.check.json' for prefix, _, _ in archives} | {sdk_receipt}
    payloads = {item for names in checksum_files.values() for item in names}
    expected = payloads | set(checksum_files) | receipts
    entries = list(directory.iterdir())
    require(all(path.is_file() and not path.is_symlink() for path in entries), 'bundle contains a link or non-file')
    actual = {path.name for path in entries}
    wanted = expected | ({name} if with_manifest else set())
    require(actual == wanted, f'bundle file set differs: missing={sorted(wanted - actual)}, extra={sorted(actual - wanted)}')
    hashes = {filename: digest(directory / filename) for filename in sorted(expected)}
    for checksum, names in checksum_files.items():
        path = directory / checksum
        require(path.stat().st_size <= 4096, 'checksum file exceeds 4 KiB')
        found = {}
        for line in path.read_text().splitlines():
            fields = line.split()
            require(len(fields) == 2 and fields[1] in names and fields[1] not in found, 'unexpected or duplicate checksum entry')
            found[fields[1]] = fields[0]
        require(found == {filename: hashes[filename] for filename in names}, 'checksum does not bind the published files')

    def receipt(filename, schema):
        value = read_json(directory / filename)
        require(value.get('schema') == schema, 'unknown package receipt schema')
        require(value.get('version') == version, 'receipt version differs from release')
        require(value.get('provenance') == identity, 'receipt source or checker commit differs')
        return value

    packages = []
    for prefix, kind, target in archives:
        filename = prefix + '.tar.gz'
        record = prefix + '.check.json'
        value = receipt(record, 'fr-release-package-check-1')
        require(value.get('kind') == kind and value.get('archive') == filename, 'receipt names another package')
        require(value.get('archive_sha256') == hashes[filename], 'receipt digest differs from package')
        checks = consumer(value.get('consumer'), version, WASM_CHECKS if kind == 'wasm' else CLI_CHECKS, wasm=kind == 'wasm')
        packages.append({'file': filename, 'kind': kind, 'target': target,
                         'sha256': hashes[filename], 'receipt': record, 'checks': checks})
    value = receipt(sdk_receipt, 'fr-sdk-package-check-2')
    require(value.get('passed') is True, 'SDK package did not pass')
    for kind, filename in [('wheel', wheel), ('sdist', sdist)]:
        part = value.get(kind)
        require(isinstance(part, dict) and part.get('name') == filename, 'SDK receipt names another distribution')
        require(part.get('sha256') == hashes[filename], 'SDK receipt digest differs from distribution')
        checks = consumer(part.get('consumer'), version, CLI_CHECKS, sdk=True)
        packages.append({'file': filename, 'kind': kind, 'sha256': hashes[filename],
                         'receipt': sdk_receipt, 'checks': checks})
    return {'schema': 'fr-product-bundle-1', 'version': version, 'provenance': identity,
            'files': hashes, 'packages': packages,
            'scope': 'Package integrity and recorded consumer checks; not complete-product or agent qualification.'}


def qualify(directory, version, source_commit, checker_commit, *, write=False):
    directory = Path(directory)
    result = inspect(directory, version, source_commit, checker_commit, with_manifest=not write)
    path = directory / manifest_name(version)
    if write:
        with path.open('x') as stream:
            stream.write(json.dumps(result, indent=2, sort_keys=True) + '\n')
    else:
        require(read_json(path) == result, 'qualification manifest differs from the checked bundle')
    return result
