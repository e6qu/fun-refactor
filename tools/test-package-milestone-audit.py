"""Reject coherent but false completion records, using a real hosted capture."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import zipfile

spec = importlib.util.spec_from_file_location('retained_packages', Path(__file__).with_name('retain-package-questions.py'))
retained = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retained)


def reject_mutation(original, mutation):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for name in ('artifact.zip', 'result.json', 'receipt.json'):
            shutil.copyfile(original.parent / name, root / name)
        receipt = json.loads((root / 'receipt.json').read_text())
        data = retained.scalar.files((root / 'artifact.zip').read_bytes(), receipt['artifact_digest'])
        value = json.loads(data['result.json'])
        row = value['projects'][0]
        if mutation == 'missing-question':
            row['cases'].pop()
        elif mutation == 'integer-for-boolean':
            row['cases'][0]['runtime'][0]['observation']['result']['value'] = 1
        elif mutation == 'false-completion':
            value['milestones_complete'] = False
        elif mutation == 'unknown-claimed-complete':
            next(cell for cell in row['cases'] if cell['case'] == 'unknown')['analysis']['result']['complete'] = True
        elif mutation == 'different-helper':
            row['project']['function'] = 'simpler_helper'
        elif mutation == 'source-rewritten':
            name = row['project']['id'] + '/upstream/' + row['project']['module'].replace('.', '/') + '.py'
            data[name] += b'\n# rewritten after checking\n'
        else:
            raise ValueError('unknown audit mutation')
        data['result.json'] = (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()
        (root / 'result.json').write_bytes(data['result.json'])
        with zipfile.ZipFile(root / 'artifact.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, content in data.items():
                archive.writestr(name, content)
        receipt['files'] = {name: hashlib.sha256((root / name).read_bytes()).hexdigest()
                            for name in ('artifact.zip', 'result.json')}
        receipt['artifact_digest'] = 'sha256:' + receipt['files']['artifact.zip']
        (root / 'receipt.json').write_text(json.dumps(receipt))
        try:
            retained.audit(root / 'result.json')
        except ValueError:
            print('Rejected ' + mutation, flush=True)
        else:
            raise AssertionError('coherent false evidence passed: ' + mutation)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    args = parser.parse_args()
    result = retained.audit(args.report)
    assert result['milestones_complete'] is True
    for mutation in ('missing-question', 'integer-for-boolean', 'false-completion',
                     'unknown-claimed-complete', 'different-helper', 'source-rewritten'):
        reject_mutation(args.report, mutation)


if __name__ == '__main__':
    main()
