"""Qualify product-task patches in the existing private grader container."""
import argparse
import gzip
import json
import os
import subprocess
from pathlib import Path

from . import product_plan as plan, selected_grading as grading, selected_local as local
from . import selected_tasks as common
from .selected_plan import ROOT, require


def controls(inputs, rust, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run private product controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    image = grading.build(output / 'image')
    result = {'schema': 'fr-product-private-controls-1', 'passed': False, 'live_model_calls': 0,
              'image': image, 'controls': {}, 'seeds': {}, 'reference_patches': {}, 'variants': {},
              'study': 'P5-initial', 'run': os.environ['GITHUB_RUN_ID'],
              'source_commit': common.comparison_grade.BASE.git(ROOT, 'rev-parse', 'HEAD').stdout.decode().strip()}
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
            result['seeds'][task] = receipt['archive_sha256']
            result['reference_patches'][task] = common.digest(directory / 'reference.patch')
            result['variants'][task] = sorted(variants)
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
        result['bindings'] = local.implementation_bindings('P5-initial')
        result['checker_bindings'] = {str(path.relative_to(ROOT)): common.digest(path)
            for path in sorted((ROOT / 'tools/agent_eval').glob('*.py'))}
        # Preserve the exact qualified image for later submitted-patch grading.
        archive = output / 'grader-image.tar.gz'
        with gzip.open(archive, 'wb') as compressed:
            process = subprocess.Popen(grading.DOCKER + ['save', image], stdout=subprocess.PIPE)
            try:
                while block := process.stdout.read(1024**2):
                    compressed.write(block)
                require(process.wait(timeout=60) == 0, 'could not retain the qualified private checker image')
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
        result['private_image_archive_sha256'] = common.digest(archive)
        from .product_runtime import validate_controls
        validate_controls(result, 'P5-initial', verify_commit=False)
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
