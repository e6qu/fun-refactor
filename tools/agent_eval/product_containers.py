"""Qualify product-task patches in the existing private grader container."""
import argparse
import json
import os
from pathlib import Path

from . import product_plan as plan, selected_grading as grading, selected_local as local
from . import selected_tasks as common
from .selected_plan import ROOT, require


def controls(inputs, rust, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run private product controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    image = grading.build(output / 'image')
    result = {'schema': 'fr-product-private-controls-1', 'passed': False, 'live_model_calls': 0,
              'image': image, 'controls': {}}
    try:
        for task in plan.TASKS['initial']:
            directory = inputs / task
            prepared = json.loads((directory / 'project.json').read_text())
            receipt = json.loads((directory / 'seed.json').read_text())
            qualified = json.loads((directory / 'controls.json').read_text())
            require(qualified['passed'] is True and qualified['task'] == task and
                    qualified['seed_sha256'] == receipt['archive_sha256'] and
                    prepared['original'] == receipt['original'], 'private controls need the qualified unchanged task')
            project = output / task / 'original'
            local.unpack(directory / 'original.tar.gz', receipt, project)
            variants = {path.stem: path for path in sorted(directory.glob('*.patch'))}
            require('reference' in variants and len(variants) >= 4, 'retain positive and negative patches')
            empty, invalid = output / task / 'empty.patch', output / task / 'invalid.patch'
            empty.write_bytes(b'')
            invalid.write_bytes(b'not a patch\n')
            variants.update(missing=None, empty=empty, invalid=invalid)
            for name, patch in variants.items():
                identity = task + '/' + name
                result['controls'][identity] = {'status': 'running'}
                checked = grading.grade(image, task, project, prepared, None, rust, patch,
                    output / task / name, milestone='P5-initial', tools={})
                require('error' not in checked and checked['passed'] == (name == 'reference'),
                        'private control did not reach its expected verdict: ' + identity)
                result['controls'][identity] = {'status': 'passed', 'accepted_change': checked['passed'],
                    'submitted_patch_sha256': checked['submitted_patch_sha256']}
        result['passed'] = True
        result['checker_bindings'] = {str(path.relative_to(ROOT)): common.digest(path)
            for path in sorted((ROOT / 'tools/agent_eval').glob('*.py'))}
        return result
    finally:
        common.save(output / 'controls.json', result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--rust', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(controls(args.inputs.resolve(), args.rust.resolve(), args.output.resolve()), indent=2))
