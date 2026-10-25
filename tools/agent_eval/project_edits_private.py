"""Check the four M2 cases inside the shared private grader before live calls."""
import argparse
import json
import os
from pathlib import Path
import tempfile

from . import project_edits_tasks as cases
from . import selected_grading as grading
from . import selected_tasks as common
from .selected_plan import require


def controls(inputs, rust, tools, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run private M2 checks on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    image = grading.build(output / 'image')
    variants = ('unchanged', 'incomplete', 'reference', 'missing-patch', 'wrong-patch')
    summary = {'passed': False, 'live_model_calls': 0, 'image': image,
        'controls': {task + '-' + variant: {'status': 'unstarted'} for task in cases.TASKS for variant in variants},
        'scope': 'Supplied M2 changes checked without runner credentials, network or writable tool mounts.'}
    try:
        for task in cases.TASKS:
            original = inputs / task / 'original'
            prepared = json.loads((inputs / task / 'preparation/project.json').read_text())
            require(prepared['task'] == task and prepared['requirement'] == cases.TASKS[task] and
                    common.sources(original) == prepared['original'], 'private control task source differs')
            for variant in variants:
                identity = task + '-' + variant
                summary['controls'][identity] = {'status': 'running'}
                with tempfile.TemporaryDirectory(prefix='fr-m2-private-control-') as temporary:
                    root = Path(temporary)
                    project = root / 'project'
                    common.checkout(original, project)
                    if variant not in {'unchanged', 'wrong-patch', 'missing-patch'}:
                        cases.prescribed_change(task, variant, project)
                    patch = root / 'change.patch'
                    patch.write_bytes(b'not a Git patch\n' if variant == 'wrong-patch' else
                                      common.comparison_grade.BASE.git(project, 'diff', '--binary', 'HEAD').stdout)
                    result = grading.grade(image, task, original, prepared, None, rust,
                        None if variant == 'missing-patch' else patch, output / identity, milestone='M2', tools=tools)
                    require(result['passed'] == (variant == 'reference'), 'unexpected private M2 outcome: ' + identity)
                    if variant in {'unchanged', 'missing-patch', 'wrong-patch'}:
                        require(result['verdict']['stage'] == 'delivery', 'invalid delivery reached candidate execution')
                    elif variant == 'incomplete':
                        require(result['verdict']['stage'] == 'source checks' and not result['verdict']['changed']['passed'],
                                'incomplete change did not reach independent validation')
                    else:
                        require(result['verdict']['undo_exact'] and result['verdict']['redo_exact'],
                                'accepted reference lacks exact reversal')
                        summary.setdefault('checker_versions', {})[task] = result['verdict']['checker_versions']
                    summary['controls'][identity] = {'status': 'passed', 'accepted_change': result['passed']}
        summary['passed'] = True
        return summary
    except Exception as error:
        summary['harness_error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        common.save(output / 'summary.json', summary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('inputs', 'rust_root', 'node', 'typescript_root', 'registry', 'output'):
        parser.add_argument('--' + name.replace('_', '-'), type=Path, required=True)
    args = parser.parse_args()
    tools = {name: getattr(args, name).resolve() for name in ('node', 'typescript_root', 'registry')}
    print(json.dumps(controls(args.inputs.resolve(), args.rust_root.resolve(), tools,
                              args.output.resolve())))
