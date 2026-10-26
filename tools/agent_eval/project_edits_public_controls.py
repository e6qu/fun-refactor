"""Check the supplied behavior command using correct and deliberately wrong code."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from . import project_edits_public as public
from . import project_edits_tasks as cases
from . import selected_local as local
from .selected_plan import require


def controls(inputs, output, cargo, rustc):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run compiled public checks on GitHub')
    require(not output.exists(), 'keep the previous control records')
    output.mkdir(parents=True)
    results = []
    summary = {'passed': False, 'live_model_calls': 0, 'environment': local.PUBLIC_CHECK_ENVIRONMENT,
               'attempt_wall_seconds': local.LIMITS['wall_seconds'],
               'client_finish_seconds': local.CLIENT_FINISH_SECONDS, 'controls': results}
    try:
        for variant in ('unchanged', 'wrong-byte-count', 'extra-allocations', 'reference'):
            started = time.time()
            directory = output / variant
            project = directory / 'project'
            shutil.copytree(inputs / 'original', project)
            if variant != 'unchanged':
                cases.prescribed_change('regex-escape-len',
                    'incomplete' if variant == 'wrong-byte-count' else 'reference', project)
            if variant == 'extra-allocations':
                path = project / 'regex-syntax/src/lib.rs'
                text = path.read_text()
                before = 'String::with_capacity(escape_len(text))'
                require(text.count(before) == 1, 'allocation control needs the reference reservation')
                path.write_text(text.replace(before, 'String::new()'))
            public.install(project)
            deadline = started + local.LIMITS['wall_seconds'] - local.CLIENT_FINISH_SECONDS
            tools = {'python': sys.executable, 'cargo': str(cargo), 'rustc': str(rustc)}
            env = {**os.environ, **local.PUBLIC_CHECK_ENVIRONMENT, 'RUSTC': str(rustc), 'CARGO_NET_OFFLINE': 'true'}
            with (directory / 'stdout').open('wb') as stdout, (directory / 'stderr').open('wb') as stderr:
                completed = subprocess.run(public.arguments(tools, deadline), cwd=project, env=env,
                                           stdout=stdout, stderr=stderr, timeout=local.LIMITS['wall_seconds'])
            error = (directory / 'stderr').read_text()
            result = {'variant': variant, 'exit_code': completed.returncode,
                      'elapsed_seconds': time.time() - started}
            results.append(result)
            if variant == 'reference':
                require(completed.returncode == 0, 'reference public behavior check failed: ' + error[-4096:])
            else:
                reason = {'unchanged': 'cannot find value `escape_len`',
                          'wrong-byte-count': 'escaped UTF-8 byte count',
                          'extra-allocations': 'escape allocated'}[variant]
                require(completed.returncode != 0 and reason in error,
                        'public check did not reject ' + variant + ' for the expected reason: ' + error[-4096:])
        summary['passed'] = True
    finally:
        (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('inputs', 'output', 'cargo', 'rustc'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    controls(args.inputs.resolve(), args.output.resolve(), args.cargo.resolve(), args.rustc.resolve())
