"""Recheck familiar project changes while preserving the original unread result."""
import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from project_change_sources import bindings, digest, save
from project_change_delivery import measure as deliver
from project_change_findings import measure as findings
from project_change_records import original, audit_delivery, audit_findings


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


scalar = load('retain-scalar-evidence')


def sources():
    names = ('tools/recheck-project-changes.py', '.github/workflows/project-changes-recheck.yml')
    return {**bindings(ROOT), **{name: digest(ROOT / name) for name in names}}


def original_record():
    catalog = json.loads((ROOT / 'tests/agent-eval/roadmap.json').read_text())
    entry, = [row for row in catalog['evidence'] if row['id'] == 'project-changes']
    path = ROOT / entry['path']
    assert path.resolve().is_relative_to(ROOT.resolve()) and digest(path) == entry['sha256']
    value = json.loads(path.read_text())
    assert value['schema'] == 'fr-project-changes' and value['passed'] is True
    assert value['unread'] == {'project': 'humanize', 'qualified': True, 'first_evaluation': True}
    freeze = path.parent / 'freeze.json'
    assert digest(freeze) == value['freeze_sha256']
    assert value['source_bindings'] == json.loads(freeze.read_text())['input_bindings']
    for name, expected in value['files'].items():
        child = path.parent / name
        assert child.resolve().is_relative_to(path.parent.resolve()) and digest(child) == expected
    return path.parent, {'path': entry['path'], 'sha256': entry['sha256'],
        'freeze_sha256': value['freeze_sha256'], 'qualified': True, 'first_evaluation': True}


def unpack(archive, destination):
    data = scalar.files(archive.read_bytes(), 'sha256:' + digest(archive))
    for name, content in data.items():
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    return data


def audit(directory, binary, alternate):
    report = json.loads((directory / 'result.json').read_text())
    assert report['schema'] == 'fr-project-changes-recheck' and report['passed'] is True
    assert report['all_projects_familiar'] is True and report['new_unread_claim'] is False
    assert report['source_bindings'] == sources()
    _, reference = original_record()
    assert report['original_unread'] == reference
    assert report['original_unread']['qualified'] is True and report['original_unread']['first_evaluation'] is True
    assert report['saved_findings'] == {'complete': True, 'projects': 3, 'changes_per_project': 7}
    assert type(report['saved_findings']['complete']) is bool
    assert type(report['saved_findings']['projects']) is int
    assert type(report['saved_findings']['changes_per_project']) is int
    assert report['deliveries'] == {'Rust': {'complete': True}, 'Python': {'complete': True}}
    assert all(type(row['complete']) is bool for row in report['deliveries'].values())
    assert report['files'] == {'replay.zip': digest(directory / 'replay.zip')}
    assert report['production_sha256'] == digest(binary) and report['alternate_sha256'] == digest(alternate)
    with tempfile.TemporaryDirectory(prefix='fr-current-project-audit-') as temporary:
        root = Path(temporary)
        unpack(directory / 'replay.zip', root)
        tasks = json.loads((root / 'tasks.json').read_text())
        assert tasks == json.loads((ROOT / 'tests/agent-eval/project-changes/tasks.json').read_text())
        assert set(path.name for path in root.iterdir()) == {'tasks.json', 'checkers.json', 'glob', 'boltons', 'humanize'}
        checkers = json.loads((root / 'checkers.json').read_text())
        assert checkers['source_bindings'] == bindings(ROOT)
        assert checkers['production']['sha256'] == digest(binary)
        assert checkers['alternate']['sha256'] == digest(alternate)
        assert checkers['control']['production']['accepted'] is True
        assert checkers['control']['alternate']['accepted'] is False
        assert checkers['invalidation_control']['invalidated'] == ['analyzer', 'declarations']
        assert checkers['invalidation_control']['independent_source_retained'] is True
        for task in tasks['projects']:
            audit_findings(task, root / task['id'], tasks['limits'], binary, alternate)
            if task['delivery'] is not None:
                audit_delivery(task, root / task['id'], tasks['limits'], binary)
    return report


def reject_false_claims(directory, binary, alternate):
    path = directory / 'result.json'
    encoded = path.read_bytes()
    value = json.loads(encoded)
    mutations = []
    for field, replacement in (('passed', False), ('all_projects_familiar', False), ('new_unread_claim', True),
                               ('source_bindings', {}), ('original_unread', {}), ('saved_findings', {}),
                               ('deliveries', {}), ('files', {})):
        changed = copy.deepcopy(value)
        changed[field] = replacement
        mutations.append((field, changed))
    changed = copy.deepcopy(value)
    changed['saved_findings']['complete'] = 1
    mutations.append(('integer completion', changed))
    try:
        for name, changed in mutations:
            save(path, changed)
            try:
                audit(directory, binary, alternate)
            except (AssertionError, KeyError, ValueError):
                print('Rejected altered ' + name, flush=True)
            else:
                raise AssertionError('accepted altered ' + name)
    finally:
        path.write_bytes(encoded)


def collect(directory, binary, alternate, checkers):
    assert os.environ.get('GITHUB_ACTIONS') == 'true', 'recheck on GitHub runners'
    before = sources()
    retained, reference = original_record()
    tasks = json.loads((ROOT / 'tests/agent-eval/project-changes/tasks.json').read_text())
    directory.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix='fr-current-project-changes-') as temporary:
        work = Path(temporary)
        save(work / 'tasks.json', tasks)
        shutil.copyfile(checkers, work / 'checkers.json')
        archives = {}
        for phase in ('findings', 'held-out'):
            identity = json.loads((retained / phase / 'identity.json').read_text())
            archives.update(scalar.files((retained / phase / 'artifact.zip').read_bytes(),
                                         identity['collection']['artifact_digest']))
        for task in tasks['projects']:
            base = work / task['id']
            source = base / 'source'
            source.mkdir(parents=True)
            for name in ('source.tar.gz', 'source.json'):
                (source / name).write_bytes(archives[task['id'] + '/source/' + name])
            tree = source / 'original'
            original(task, base, tree, tasks['limits'])
            findings(task, tree, base / 'findings', binary, alternate)
            if task['delivery'] is not None:
                deliver(task, tree, base / 'delivery', binary)
            shutil.rmtree(tree)
        with zipfile.ZipFile(directory / 'replay.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(work.rglob('*')):
                if path.is_file():
                    assert not path.is_symlink()
                    archive.write(path, path.relative_to(work).as_posix())
    assert sources() == before, 'replay changed its source inputs'
    save(directory / 'result.json', {'schema': 'fr-project-changes-recheck', 'passed': True,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_bindings': before, 'all_projects_familiar': True, 'new_unread_claim': False,
        'original_unread': reference, 'saved_findings': {'complete': True, 'projects': 3, 'changes_per_project': 7},
        'deliveries': {'Rust': {'complete': True}, 'Python': {'complete': True}},
        'production_sha256': digest(binary), 'alternate_sha256': digest(alternate),
        'files': {'replay.zip': digest(directory / 'replay.zip')}})
    audit(directory, binary, alternate)
    reject_false_claims(directory, binary, alternate)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    previous = [row for row in catalog['evidence'] if row['id'] == 'current-project-changes']
    assert len(previous) <= 1
    catalog['evidence'] = [row for row in catalog['evidence'] if row['id'] != 'current-project-changes']
    relative = str((directory / 'result.json').relative_to(ROOT))
    catalog['evidence'].append({'id': 'current-project-changes', 'path': relative, 'sha256': digest(directory / 'result.json'),
        'kind': 'deterministic', 'audit': 'python3 tools/recheck-project-changes.py --audit ' + str(directory.relative_to(ROOT))
            + ' --fr target/project-change-checkers/production-fr --alternate target/project-change-checkers/alternate-fr',
        'scope': 'Current replay of all 21 change controls and both renames. All projects are familiar; the original first Humanize result remains separate.'})
    for gate in catalog['obligations']:
        if gate['id'] in ('C.repository-mutations', 'D.structural'):
            for requirement in gate['requires']:
                assert requirement['evidence'] in ('project-changes', 'current-project-changes')
                requirement['evidence'] = 'current-project-changes'
                for assertion in requirement['assertions']:
                    if assertion['pointer'] == '/unread/qualified':
                        assertion['pointer'] = '/original_unread/qualified'
    save(catalog_path, catalog)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument('--collect', type=Path)
    action.add_argument('--audit', type=Path)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--alternate', type=Path, required=True)
    parser.add_argument('--checkers', type=Path)
    args = parser.parse_args()
    if args.collect:
        assert args.checkers is not None, 'declare the recorded checker control'
        collect(args.collect.resolve(), args.fr.resolve(), args.alternate.resolve(), args.checkers.resolve())
    else:
        audit(args.audit.resolve(), args.fr.resolve(), args.alternate.resolve())
