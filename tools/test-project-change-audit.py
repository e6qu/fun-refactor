"""Reject false completion claims even when their local file hashes are renewed."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('project_retention', ROOT / 'tools/retain-project-changes.py')
retained = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retained)


def reject(original, mutation, binary, alternate):
    with tempfile.TemporaryDirectory(prefix='fr-false-project-record-') as temporary:
        directory = Path(temporary) / 'record'
        shutil.copytree(original, directory)
        path = directory / 'result.json'
        report = json.loads(path.read_text())
        if mutation == 'integer-for-boolean':
            report['saved_findings']['complete'] = 1
        elif mutation == 'missing-project':
            report['saved_findings']['projects'] = 2
        elif mutation == 'missing-language':
            del report['deliveries']['Python']
        elif mutation == 'unread-not-qualified':
            report['unread']['qualified'] = False
        elif mutation == 'removed-failure':
            report['failures'].remove(next(iter(json.loads(retained.FREEZE.read_text())['development_failures'])))
        elif mutation == 'changed-published-package-result':
            package = directory / 'prior-questions/files/result.json'
            value = json.loads(package.read_text())
            value['projects'][0]['cases'].pop()
            package.write_text(json.dumps(value))
        else:
            raise ValueError(mutation)
        report['files'] = {name: retained.digest(directory / name) for name in report['files']}
        path.write_text(json.dumps(report))
        try:
            retained.audit(directory, binary, alternate)
        except (AssertionError, ValueError, KeyError):
            print('Rejected ' + mutation, flush=True)
        else:
            raise AssertionError('false project completion passed: ' + mutation)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--alternate', type=Path, required=True)
    args = parser.parse_args()
    retained.audit(args.report.parent, args.fr, args.alternate)
    for mutation in ('integer-for-boolean', 'missing-project', 'missing-language',
                     'unread-not-qualified', 'removed-failure', 'changed-published-package-result'):
        reject(args.report.parent, mutation, args.fr, args.alternate)
