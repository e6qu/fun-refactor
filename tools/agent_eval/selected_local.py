"""Run the selected native CLIs locally; keep independent grading on GitHub."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import time

from .bounded_host import run as bounded_run
from .selected_cli import configuration, invocation, export_session
from . import selected_tasks as tasks
from .selected_plan import DIRECTORY, ROOT, encoded, prepare, require

LIMITS = {'wall_seconds': 150, 'rss_bytes': 768 * 1024**2, 'disk_bytes': 512 * 1024**2,
          'cpu_limit_seconds': 75, 'transcript_bytes': 1024**2}
CHECKS = {
    'boltons-rename': ['python3 -B -m pytest -q -p no:cacheprovider tests/test_mathutils.py',
                       'python3 -B -m doctest boltons/mathutils.py'],
    'unicode-dice': ['cargo test --locked --offline'],
    'access-proof': ['lake -d specs build'],
}


def hash_file(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity():
    require(not tasks.comparison_grade.BASE.git(ROOT, 'status', '--porcelain', '--untracked-files=no').stdout,
            'freeze and run only committed M1 code')
    return {key: tasks.comparison_grade.BASE.git(ROOT, 'rev-parse', argument).stdout.decode().strip()
            for key, argument in [('commit', 'HEAD'), ('tree', 'HEAD^{tree}')]}


def prompt(task, setup):
    return (tasks.TASKS[task]['request'] + '\n\n'
        'Complete this task independently in the current project. Ordinary reading, search, editing and shell tools '
        'are available. Do not browse the Internet, inspect the experiment or other attempts, delegate, call another '
        'model, commit, push, or change dependencies or checking settings. Do not inspect account stores or host configuration. '
        'The entire attempt and its child commands already run under the workstation resource guard. '
        'Do not start a nested guard, increase limits, or run full fr builds or repository-wide gates. '
        'Use only small checks for this task. Keep failed checks and limitations in your report.\n'
        + ('Fr and its project skill are installed. Read .agents/skills/fr/SKILL.md, then use fr when useful.\n'
           if setup == 'fr' else 'Fr is not provided in this setup. Use the ordinary tools.\n')
        + 'Available project checking commands:\n' + '\n'.join(CHECKS[task]) + '\n'
        'Finish by writing change.patch with git diff --binary HEAD, and report what changed and which checks passed. '
        'An unfinished check does not count as passing. Do not claim a model proof proves the original program.\n')


def unpack(archive, receipt, project):
    require(hash_file(archive) == receipt['archive_sha256'], 'task archive differs from its hosted receipt')
    project.mkdir(parents=True, exist_ok=False)
    total = 0
    with tarfile.open(archive) as source:
        members = source.getmembers()
        require(len(members) == len(receipt['original']) <= 4096, 'task archive file count differs')
        seen = set()
        for member in members:
            path = Path(member.name)
            require(not path.is_absolute() and '..' not in path.parts and member.isfile() and
                    member.name in receipt['original'] and member.name not in seen, 'unsafe or repeated task archive entry')
            seen.add(member.name)
            total += member.size
            require(total <= 32 * 1024**2, 'task archive exceeds 32 MiB')
            target = project / path
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(member) as data:
                target.write_bytes(data.read())
            target.chmod(member.mode)
    require(tasks.sources(project) == receipt['original'], 'task source or modes differ from the hosted original')


def freeze(inputs, binaries, output):
    require(sys.platform == 'darwin', 'this execution plan is for the requested local macOS CLIs')
    output.mkdir(parents=True, exist_ok=False)
    proposed = prepare(ROOT)
    paths = {name: {'path': str(path.resolve()), 'sha256': hash_file(path.resolve())} for name, path in binaries.items()}
    companions = {'codex-code-mode-host': binaries['codex'].resolve().parent / 'codex-code-mode-host',
                  'opencode': binaries['opencode2'].resolve().parent / 'opencode'}
    components = {name: {'path': str(path), 'sha256': hash_file(path)} for name, path in companions.items()}
    versions = {}
    for name, entry in paths.items():
        result = subprocess.run([entry['path'], '--version'], capture_output=True, text=True, timeout=10)
        require(result.returncode == 0, 'selected CLI version probe failed')
        versions[name] = result.stdout.strip()
    require(versions == {'codex': 'codex-cli 0.164.0', 'opencode2': 'opencode v2.0.32', 'fr': 'fr 0.84.2'},
            'installed CLI releases differ from the reviewed local setup: ' + json.dumps(versions))
    seeds = {}
    for task in tasks.TASKS:
        directory = inputs / 'm1-tasks' / task
        receipt = json.loads((directory / 'seed.json').read_text())
        require(receipt['task'] == task and hash_file(directory / 'original.tar.gz') == receipt['archive_sha256'],
                'hosted task seed differs')
        seeds[task] = {'archive': str((directory / 'original.tar.gz').resolve()), 'receipt': receipt}
    packets = {row['id']: prompt(row['task'], row['setup']) for row in proposed['attempts']}
    value = {'source': identity(), 'schedule': proposed, 'binaries': paths, 'components': components, 'versions': versions,
             'seeds': seeds, 'prompts': packets,
             'limits': LIMITS, 'outer_guard': '/Users/zardoz/.codex/tools/fr-local-guard.py',
             'execution': 'User-requested local Codex and OpenCode2 CLIs with their existing account stores. No credential transfer.',
             'accounting': {'actual_cost': None, 'estimated_cost': None,
                'provider_request_cap_enforced': False, 'token_cap_enforced': False,
                'explanation': 'Native CLI output is recorded. These CLIs do not expose a shared hard token/request cap. '
                               'Enforced limits are wall time, sampled memory/CPU/disk/output and the outer workstation guard.'}}
    value['sha256'] = hashlib.sha256(encoded(value)).hexdigest()
    tasks.save(output / 'plan.json', value)
    return value


def usage(client, events):
    if client == 'codex':
        completed = [row['usage'] for row in events if row.get('type') == 'turn.completed']
        return {'reported': completed, 'complete_turns': len(completed), 'actual_cost': None,
                'model_observed': False, 'explanation': 'The CLI turn totals do not report observed model identity or billed charges.'}
    steps = [row['part'] for row in events if row.get('type') == 'step_finish']
    return {'reported': [{'tokens': row['tokens'], 'catalog_cost': row['cost']} for row in steps],
            'complete_steps': len(steps), 'actual_cost': None,
            'explanation': 'A zero catalog cost is not an observed account charge.'}


def attempt(plan_path, attempt_id):
    require(sys.platform == 'darwin', 'run the requested local CLI attempts on macOS')
    value = json.loads(plan_path.read_text())
    unsigned = {key: item for key, item in value.items() if key != 'sha256'}
    require(hashlib.sha256(encoded(unsigned)).hexdigest() == value['sha256'], 'local execution plan changed')
    require(value['source'] == identity() and value['schedule'] == prepare(ROOT), 'committed study inputs changed')
    require(value['limits'] == LIMITS, 'local limits changed')
    for binary in [*value['binaries'].values(), *value['components'].values()]:
        require(hash_file(Path(binary['path'])) == binary['sha256'], 'a selected native binary changed')
    rows = [row for row in value['schedule']['attempts'] if row['id'] == attempt_id]
    require(len(rows) == 1, 'attempt is not in the fixed 36-run schedule')
    row = rows[0]
    output = plan_path.parent / 'attempts' / attempt_id
    output.mkdir(parents=True, exist_ok=False)
    report = {'id': attempt_id, 'status': 'started', 'plan_sha256': value['sha256'], 'profile': row['profile'],
              'task': row['task'], 'setup': row['setup'], 'independent_outcome': None,
              'actual_cost': None, 'started_at': time.time(), 'scope': 'Local native CLI attempt; independently grade on GitHub.'}
    tasks.save(output / 'result.json', report)
    started = time.monotonic()
    try:
        project = Path('/private/tmp') / ('fr-m1-' + value['sha256'][:12]) / attempt_id / 'project'
        seed = value['seeds'][row['task']]
        unpack(Path(seed['archive']), seed['receipt'], project)
        tasks.comparison_grade.BASE.initialize(project)
        binary_directory = project.parent / 'bin'
        binary_directory.mkdir()
        if row['setup'] == 'fr':
            (binary_directory / 'fr').symlink_to(value['binaries']['fr']['path'])
            shutil.copytree(ROOT / 'skills/fr', project / '.agents/skills/fr')
        env = {key: os.environ[key] for key in ('HOME', 'PATH', 'LANG', 'CODEX_HOME', 'XDG_CONFIG_HOME',
            'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME', 'SSL_CERT_FILE', 'CODEX_CA_CERTIFICATE') if key in os.environ}
        env.update(PATH=str(binary_directory) + os.pathsep + env['PATH'], PYTHONDONTWRITEBYTECODE='1',
            CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
            CARGO_NET_OFFLINE='true', FR_LEAN_JOBS='1', LEAN_NUM_THREADS='1', RAYON_NUM_THREADS='1',
            RUST_TEST_THREADS='1', OPENCODE_DISABLE_AUTOUPDATE='1', OPENCODE_DISABLE_LSP_DOWNLOAD='1',
            OPENCODE_DISABLE_DEFAULT_PLUGINS='1', OPENCODE_DISABLE_MODELS_FETCH='1', DISABLE_TELEMETRY='1',
            GOMAXPROCS='1', BUN_JSC_useJIT='false')
        require((shutil.which('fr', path=env['PATH']) is not None) == (row['setup'] == 'fr'), 'wrong fr availability')
        profile = value['schedule']['profiles'][row['profile']]
        if profile['client'] == 'opencode2':
            config = configuration(profile, row['setup'], project)
            env['OPENCODE_CONFIG_CONTENT'] = json.dumps(config)
            tasks.save(output / 'requested-config.json', config)
        prompt_text = value['prompts'][attempt_id]
        (output / 'prompt.txt').write_text(prompt_text)
        argv, stdin = invocation(profile, value['binaries'], project, output, prompt_text, 'workspace-write')
        tasks.save(output / 'command.json', argv)
        report['project'] = str(project)
        tasks.save(output / 'result.json', report)
        with (output / 'native.stdout').open('xb') as stdout, (output / 'native.stderr').open('xb') as stderr:
            process = bounded_run(argv, stdin, stdout, stderr, project, env=env, cwd=project, **LIMITS)
        report['process'] = process
        events = []
        for line in (output / 'native.stdout').read_bytes().splitlines():
            try:
                events.append(json.loads(line))
            except ValueError:
                report['incomplete_json_lines'] = report.get('incomplete_json_lines', 0) + 1
        report['usage'] = usage(profile['client'], events)
        report['status'] = 'completed' if process['exit_code'] == 0 and not process['stop_reason'] else 'failed'
        if profile['client'] == 'opencode2' and events:
            session = argv[argv.index('--session') + 1]
            require(all(event['sessionID'] == session for event in events), 'native stream belongs to another session')
            saved = export_session(value['binaries']['opencode2']['path'], session, project, env, output)
            require(saved['info']['id'] == session and saved['info']['model'] == profile['model'] and
                    saved['info']['agent'] == 'build' and saved['info']['permissions'] == config['permissions'],
                    'native exported model, agent or permissions differ')
            assistants = [message['info'] for message in saved['messages'] if message['info']['role'] == 'assistant']
            require(all(message['providerID'] == profile['model']['providerID'] and message['modelID'] == profile['model']['id']
                        for message in assistants), 'native model response substituted another model')
            tasks.save(output / 'session-export.json', saved)
            report['usage']['model_observed'] = bool(assistants)
            report['usage']['session_model'] = saved['info']['model']
            if saved['info']['outcome'] != 'succeeded':
                report['status'] = 'failed'
        elif profile['client'] == 'codex' and report['usage']['complete_turns'] != 1:
            report['status'] = 'failed'
        report['final_source'] = tasks.sources(project)
        patch = project / 'change.patch'
        report['patch_delivered'] = patch.is_file() and not patch.is_symlink()
        if report['patch_delivered']:
            require(patch.stat().st_size <= 1024**2, 'delivered patch exceeds 1 MiB')
            shutil.copyfile(patch, output / 'change.patch')
        # Collection archives files. Independent grading runs on GitHub.
        with tarfile.open(output / 'candidate.tar.gz', 'w:gz') as archive:
            for name in sorted(report['final_source']):
                archive.add(project / name, arcname=name, recursive=False)
        report['candidate_archive_sha256'] = hash_file(output / 'candidate.tar.gz')
        report['fr_contribution'] = None
        return report
    except Exception as error:
        report.update(status='failed', error=type(error).__name__ + ': ' + str(error))
        raise
    finally:
        report['elapsed_seconds'] = time.monotonic() - started
        tasks.save(output / 'result.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    freezing = commands.add_parser('freeze')
    freezing.add_argument('--inputs', type=Path, required=True)
    freezing.add_argument('--fr', type=Path, required=True)
    freezing.add_argument('--codex', type=Path, required=True)
    freezing.add_argument('--opencode2', type=Path, required=True)
    freezing.add_argument('--output', type=Path, required=True)
    running = commands.add_parser('attempt')
    running.add_argument('--plan', type=Path, required=True)
    running.add_argument('--id', required=True)
    args = parser.parse_args()
    if args.operation == 'freeze':
        result = freeze(args.inputs.resolve(), {name: getattr(args, name).resolve() for name in ('fr', 'codex', 'opencode2')},
                        args.output.resolve())
        print(json.dumps({'plan_sha256': result['sha256'], 'attempts': len(result['schedule']['attempts'])}))
    else:
        result = attempt(args.plan.resolve(), args.id)
        print(json.dumps({key: result[key] for key in ('id', 'status', 'elapsed_seconds', 'independent_outcome')}))
