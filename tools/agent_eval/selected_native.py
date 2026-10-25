"""Run the six selected app/tool exchanges on GitHub, without external model access."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import uuid

from .container_resources import ContainerResources, audit, limits
from .isolated_grade import DOCKER, invoke
from .selected_native_probe import packages
from .selected_plan import ROOT, DIRECTORY, require

spec = importlib.util.spec_from_file_location('selected_container_boundary', ROOT / 'tools/agent-comparison-rehearsal.py')
boundary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boundary)
MODULES = ('selected_native_probe.py', 'selected_provider.py', 'selected_plan.py', 'codex_client.py', 'opencode2_client.py')


def save(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def build(client, setup, binary, fr, output):
    context = output / 'context'
    (context / 'client/bin').mkdir(parents=True)
    shutil.copy2(binary, context / 'client/bin/client')
    (context / 'tool/bin').mkdir(parents=True)
    (context / 'skill').mkdir()
    if setup == 'fr':
        shutil.copy2(fr, context / 'tool/bin/fr')
        shutil.copytree(ROOT / 'skills/fr', context / 'skill', dirs_exist_ok=True)
    (context / 'agent_eval').mkdir()
    for name in MODULES:
        shutil.copyfile(ROOT / 'tools/agent_eval' / name, context / 'agent_eval' / name)
    for name in ('clients.json', 'profiles.json'):
        shutil.copyfile(ROOT / DIRECTORY / name, context / name)
    shutil.copyfile(ROOT / 'tools/selected-agent-probe.py', context / 'selected-agent-probe.py')
    shutil.copyfile(ROOT / DIRECTORY / 'Native.Dockerfile', context / 'Dockerfile')
    tag = 'fr-m1-' + client + '-' + setup
    result = subprocess.run(DOCKER + ['build', '--tag', tag, str(context)], capture_output=True, timeout=120)
    (output / 'build.stdout').write_bytes(result.stdout)
    (output / 'build.stderr').write_bytes(result.stderr)
    require(result.returncode == 0, 'native control image did not build; inspect build.stderr')
    image = subprocess.check_output(DOCKER + ['image', 'inspect', '--format', '{{.Id}}', tag], timeout=10).decode().strip()
    require(image.startswith('sha256:') and len(image) == 71, 'missing immutable native image identity')
    shutil.rmtree(context)
    save(output / 'image.json', {'image': image, 'client': client, 'setup': setup})
    return image


def exchange(image, profile, setup, output):
    output.mkdir(parents=True)
    native = output / 'native'
    native.mkdir(mode=0o777)
    native.chmod(0o777)  # The fresh unprivileged container owns only these scripted diagnostics.
    require(',' not in str(native), 'container diagnostics path must not contain a comma')
    budget = {**limits(), 'memory_bytes': 512 * 1024**2, 'cpu_seconds': 60}
    name = 'fr-m1-' + uuid.uuid4().hex
    started = time.monotonic()
    report = {'profile': profile, 'setup': setup, 'image': image, 'passed': False, 'live_model_calls': 0,
        'scope': 'Scripted native client/tool integration. Whole-container limits and shutdown; not live outcomes or costs.'}
    with boundary.isolated_budget(output, budget) as slice_id:
        scope = ContainerResources(budget, slice_id, output).start()

        def docker(arguments, timeout):
            process, stdout, stderr = scope.invoke(DOCKER + arguments, b'', output, timeout, 2 * 1024**2)
            report.setdefault('commands', []).append({'arguments': arguments, 'process': process,
                'stdout': stdout.decode(errors='replace'), 'stderr': stderr.decode(errors='replace')})
            require(process['exit_code'] == 0 and not process['stop_reason'], 'native container command failed: ' + str(arguments))
            return stdout

        try:
            docker(['create', '--name', name, '--pull=never', '--init', '--network=none', '--read-only',
                '--cap-drop=ALL', '--security-opt=no-new-privileges', '--user=65534:65534',
                '--cpus=0.5', '--memory=512m', '--memory-swap=512m', '--pids-limit=64', '--log-driver=none',
                '--workdir=/workspace', '--env=HOME=/home/control/home',
                '--tmpfs=/workspace:rw,exec,nosuid,nodev,size=256m,uid=65534,gid=65534,mode=0700',
                '--tmpfs=/tmp:rw,exec,nosuid,nodev,size=64m,mode=1777',
                '--tmpfs=/home/control:rw,exec,nosuid,nodev,size=64m,uid=65534,gid=65534,mode=0700',
                '--mount=type=bind,src=' + str(native) + ',dst=/diagnostics',
                image, 'python3', '-c', 'import time; time.sleep(150)'], 10)
            docker(['start', name], 10)
            data = docker(['exec', name, 'python3', '-B', '-m', 'agent_eval.selected_native_probe',
                           '--profile', profile, '--setup', setup], 90)
            report['native'] = json.loads(data)
            require(report['native']['passed'], 'native exchange did not pass')
            remaining = docker(['top', name, '-eo', 'pid,comm'], 5).decode().splitlines()
            report['after_client_exit'] = remaining
            require(len(remaining) == 3, 'client left workers after shutdown; only init and the idle container should remain')
            scope.check()
        except Exception as error:
            report['error'] = type(error).__name__ + ': ' + str(error)
        finally:
            cleanup, stdout, stderr = invoke(DOCKER + ['rm', '--force', name], b'', output, 10, 65536)
            report['cleanup'] = {'process': cleanup, 'stdout': stdout.decode(errors='replace'), 'stderr': stderr.decode(errors='replace')}
            report['resources'] = scope.finish()
            report['elapsed_seconds'] = time.monotonic() - started
        try:
            checked = audit(report['resources'], budget)
            require('error' not in report, report.get('error', ''))
            require(cleanup['exit_code'] == 0 and not cleanup['stop_reason'] and checked['complete'], 'container cleanup incomplete')
            require(checked['stop_reason'] is None, 'container exceeded its frozen control budget')
            require(checked['cpu_seconds'] > 0 and checked['memory_peak_bytes'] > 0, 'container counters missing')
            report['passed'] = True
        finally:
            save(output / 'result.json', report)
    return report


def run(binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run native controls on GitHub under the workstation rules')
    require(binary.is_file(), 'the checked fr executable is required')
    output.mkdir(parents=True, exist_ok=False)
    profiles = json.loads((ROOT / DIRECTORY / 'profiles.json').read_text())
    clients = json.loads((ROOT / DIRECTORY / 'clients.json').read_text())
    summary = {'passed': False, 'live_model_calls': 0, 'exchanges': {
        name + '-' + setup: {'status': 'unstarted'} for name in profiles for setup in ('files', 'fr')}}
    try:
        images = {}
        for name, client in clients.items():
            executable = packages.install(client, output / ('package-' + name))
            for setup in ('files', 'fr'):
                directory = output / ('image-' + name + '-' + setup)
                directory.mkdir()
                images[name, setup] = build(name, setup, executable, binary, directory)
        for name, profile in profiles.items():
            for setup in ('files', 'fr'):
                identity = name + '-' + setup
                summary['exchanges'][identity] = {'status': 'running'}
                result = exchange(images[profile['client'], setup], name, setup, output / identity)
                summary['exchanges'][identity] = {'status': 'passed', 'image': result['image']}
        summary['passed'] = True
        return summary
    except Exception as error:
        for row in summary['exchanges'].values():
            if row['status'] == 'running':
                row.update(status='failed', error=type(error).__name__ + ': ' + str(error))
        summary['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        save(output / 'summary.json', summary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.fr.resolve(), args.output.resolve())))
