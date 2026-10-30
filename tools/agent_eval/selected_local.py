"""Run the selected native CLIs locally; keep independent grading on GitHub."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import sys
import tarfile
import time
import tomllib

from .bounded_host import run as bounded_run
from .bounded_host import elapsed_time
from .selected_cli import configuration, invocation, export_session, verify_export
from . import selected_tasks as tasks
from .selected_plan import CASE_DIRECTORIES, DIRECTORY, ROOT, encoded, prepare, require

LIMITS = {'wall_seconds': 150, 'rss_bytes': 768 * 1024**2, 'disk_bytes': 512 * 1024**2,
          'cpu_limit_seconds': 75, 'transcript_bytes': 1024**2}
CLIENT_FINISH_SECONDS = 10
TOOL_DEADLINE_MARKER = '<ATTEMPT_TOOL_DEADLINE_UNIX>'
PUBLIC_CHECK_ENVIRONMENT = {'CARGO_BUILD_JOBS': '1', 'CARGO_INCREMENTAL': '0',
    'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_DEV_OPT_LEVEL': '0', 'CARGO_PROFILE_TEST_DEBUG': '0'}


def hash_file(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity():
    require(not tasks.comparison_grade.BASE.git(ROOT, 'status', '--porcelain', '--untracked-files=no').stdout,
            'freeze and run only committed milestone code')
    result = {key: tasks.comparison_grade.BASE.git(ROOT, 'rev-parse', argument).stdout.decode().strip()
              for key, argument in [('commit', 'HEAD'), ('tree', 'HEAD^{tree}')]}
    paths = [line.rstrip('/') for line in (ROOT / 'native-source-inputs.txt').read_text().splitlines()
             if line and not line.startswith('!')]
    result['native_inputs'] = {name: tasks.comparison_grade.BASE.git(ROOT, 'rev-parse', 'HEAD:' + name).stdout.decode().strip()
                               for name in paths}
    return result


def implementation_bindings(milestone):
    paths = list((ROOT / 'tools').rglob('*.py')) + list((ROOT / 'sdk/python/src').rglob('*.py'))
    paths += [path for path in (ROOT / 'skills/fr').rglob('*') if path.is_file()]
    paths += [path for path in (ROOT / DIRECTORY).iterdir() if path.is_file() and path.name != 'README.md']
    paths += [ROOT / '.github/actions/lean-toolchain/action.yml', ROOT / '.github/actions/native-tools/python-wheels.txt']
    if milestone in {'M2', 'M3'}:
        directory = ROOT / CASE_DIRECTORIES[milestone]
        paths += [directory / 'tasks.json', directory / 'checkers.json']
        if milestone == 'M3':
            paths += [directory / 'cases.json', directory / 'profiles.json']
            paths += list((directory / 'controls').glob('*.lean'))
        paths += [path for path in (directory / 'fixtures').rglob('*') if path.is_file()]
    return {str(path.relative_to(ROOT)): hash_file(path) for path in sorted(paths)}


def m2_checks(task, tools, deadline_unix):
    if task == 'regex-escape-len':
        from . import project_edits_public as checker
        return [{'name': 'escape-behavior', 'argv': checker.arguments(tools, deadline_unix)}]
    from . import project_edits_tasks as cases
    return cases.public_checks(task, tools)


def checking_input_changes(project, expected):
    changed = []
    for name, digest in expected.items():
        path = project / name
        try:
            preserved = not path.parent.is_symlink() and not path.is_symlink() and path.is_file() and hash_file(path) == digest
        except OSError:
            preserved = False
        if not preserved:
            changed.append(name)
    return sorted(changed)


def prompt(task, setup, checkers, milestone):
    if milestone == 'M1':
        python, cargo, lake = [shlex.quote(checkers[name]['path']) for name in ('python', 'cargo', 'lake')]
        checks = {'boltons-rename': [python + ' -B -m pytest -q -p no:cacheprovider tests/test_mathutils.py',
                                    python + ' -B -m doctest boltons/mathutils.py'],
                  'unicode-dice': [cargo + ' test --locked --offline'],
                  'access-proof': [lake + ' -d specs build']}[task]
        request = tasks.TASKS[task]['request']
    elif milestone == 'M3':
        from .proof_writing_tasks import DIRECTORY as proof_directory
        requirements = json.loads((ROOT / proof_directory / 'tasks.json').read_text())['tasks'][task]
        request = requirements['request']
        if 'arithmetic' in requirements:
            request += '\n\n' + requirements['arithmetic']
        request += ('\n\nLean checks a theorem under its definitions and assumptions. Tactics are the proof steps you write. '
            'The installed Lean includes its standard library, not Mathlib. No proof steps are supplied. '
            f'The standard library source folder is {shlex.quote(checkers["lean"]["source_root"])}. '
            f'Its integer definitions are in {shlex.quote(str(Path(checkers["lean"]["source_root"]) / "Init/Data/Int"))}. '
            f'Use {shlex.quote(checkers["lean"]["path"])} and the supplied Lake command. '
            'Search names from the supplied model when looking up definitions or existing library proofs. '
            f'The combined CLI transcript, including command output, is limited to {LIMITS["transcript_bytes"]:,} bytes. '
            'Search narrow paths and cap printed matches, for example rg -n "name" FILE | head -40. '
            'Keep working files under .fr/, or remove them before delivery. Only the marked proof region may change. '
            'A fr proof-check report with passed=false is a failed check even if the command exits successfully. '
            'Write a draft and check it early, then use Lean diagnostics to repair it. '
            'Repair rejected tactics using the reported diagnostics; keep the theorem and definitions fixed.')
        checks = [shlex.join([checkers['lake']['path'], '-d', 'specs', 'build', '--wfail'])]
    else:
        require(milestone == 'M2', 'choose an explicit milestone for the attempt prompt')
        from . import project_edits_tasks as cases
        checks = [shlex.join(row['argv']) for row in m2_checks(task,
                  {name: value['path'] for name, value in checkers.items()}, TOOL_DEADLINE_MARKER)]
        request = cases.TASKS[task]['request']
        request = ('Complete every requirement:\n' + '\n'.join(
            '- ' + sentence + '.' for sentence in request.removesuffix('.').split('. ')))
    checking_files = ''
    if milestone in {'M2', 'M3'}:
        checking_files = 'Preserve provided .fr/checks.json'
        if milestone == 'M2' and task == 'regex-escape-len':
            checking_files += ' and .fr/check-escape'
        checking_files += ', including during cleanup; changing or deleting them makes the attempt fail. '
    return (request + '\n\n'
        f'The whole attempt has a {LIMITS["wall_seconds"]}-second deadline, including preparation, '
        'model work, tools, checks, patch delivery and final session accounting. '
        'Leave time to deliver change.patch and finish the session. '
        'Review the complete patch against every requested requirement. A compile check alone does not establish '
        'the requested result. Keep requested documentation concise; omit optional extra examples. '
        'Run the supplied checks once; do not repeat checks already passed by fr workflow or add optional checks before delivery. '
        'Complete this task independently in the current project. Ordinary reading, search, editing and shell tools '
        'are available. Do not browse the Internet, inspect the experiment or other attempts, delegate, call another '
        'model, commit, push, or change dependencies or checking settings. Do not inspect account stores or host configuration. '
        'The entire attempt and its child commands already run under the workstation resource guard. '
        'Do not start a nested guard, increase limits, or run full fr builds or repository-wide gates. '
        'Run tool commands serially, keeping at most one compiler or fr command active. '
        'Use only small checks for this task. Keep failed checks and limitations in your report. '
        'Remove only temporary files you created. ' + checking_files +
        'Leave unrelated project files unchanged.\n'
        + ('Fr and its project skill are installed. Read .agents/skills/fr/SKILL.md. '
           'Use fr for a contributing discovery, preview, edit or check step in the delivered change. '
           'Ordinary tools remain available for the rest.\n'
           if setup == 'fr' else 'Fr is not provided in this setup. Use the ordinary tools.\n')
        + 'Available project checking commands:\n' + '\n'.join(checks) + '\n'
        'Finish by writing change.patch with git diff --binary HEAD, and report what changed and which checks passed. '
        'An unfinished check does not count as passing. Do not claim a model proof proves the original program.\n')


def execution_prompt(frozen_prompt, started_at, wall_seconds, finish_seconds):
    require(0 < finish_seconds < wall_seconds, 'reserve explicit client finishing time within the attempt deadline')
    deadline = started_at + wall_seconds
    return (f'Attempt clock (Unix seconds): start {started_at:.3f}; '
        f'finish tools by {deadline - finish_seconds:.3f}; '
        f'finish the response and session accounting by {deadline:.3f}.\n'
        'Use date +%s alongside necessary shell commands to check remaining time. '
        'Keep the final response concise.\n\n' +
        frozen_prompt.replace(TOOL_DEADLINE_MARKER, f'{deadline - finish_seconds:.3f}'))


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


def verified_fr_build(source, executable_sha256, build):
    commit = (build / 'source-commit.txt').read_text().strip()
    tree = (build / 'source-tree.txt').read_text().strip()
    require(len(commit) == len(tree) == 40 and all(character in '0123456789abcdef' for character in commit + tree),
            'fr needs its hosted source commit and tree')
    built_inputs = {name: tasks.comparison_grade.BASE.git(ROOT, 'rev-parse', commit + ':' + name).stdout.decode().strip()
                    for name in source['native_inputs']}
    require(built_inputs == source['native_inputs'] and
            tasks.comparison_grade.BASE.git(ROOT, 'rev-parse', commit + '^{tree}').stdout.decode().strip() == tree and
            (build / 'fr.sha256').read_text().split()[0] == executable_sha256,
            'fr executable or hosted native source differs from this study')
    return {'source_commit': commit, 'source_tree': tree, 'native_inputs': built_inputs,
            'executable_sha256': executable_sha256}


def freeze_binaries(binaries, output):
    require(set(binaries) == {'fr', 'codex', 'opencode2'}, 'supply all three native executables explicitly')
    companions = {'codex-code-mode-host': binaries['codex'].resolve().parent / 'codex-code-mode-host',
                  'opencode': binaries['opencode2'].resolve().parent / 'opencode'}
    directory = output / 'bin'
    directory.mkdir(parents=True, exist_ok=False)
    captured = {}
    for name, source in {**binaries, **companions}.items():
        source = source.resolve()
        expected = hash_file(source)
        target = directory / name
        shutil.copyfile(source, target)
        target.chmod(source.stat().st_mode & 0o777)
        require(hash_file(target) == expected and hash_file(source) == expected,
                'native executable changed while capturing the frozen copy: ' + name)
        captured[name] = {'path': str(target.resolve()), 'sha256': expected}
    return ({name: captured[name] for name in binaries}, {name: captured[name] for name in companions})


def freeze(inputs, binaries, runtime, output, milestone):
    require(sys.platform == 'darwin', 'this execution plan is for the requested local macOS CLIs')
    output.mkdir(parents=True, exist_ok=False)
    proposed = prepare(ROOT, milestone)
    require(LIMITS == {name: proposed['requirements']['limits'][name] for name in LIMITS},
            'execution limits differ from the frozen task requirements')
    expected_runtime = {'python', 'rust_root'} | ({'lean_root'} if milestone in {'M1', 'M3'} else
                                                {'node', 'typescript', 'cargo_home', 'fr_build', 'controls'})
    if milestone == 'M3':
        expected_runtime |= {'fr_build', 'controls'}
    require(set(runtime) == expected_runtime, 'supply exactly the explicit checkers for ' + milestone)
    paths, components = freeze_binaries(binaries, output)
    versions = {}
    for name, entry in paths.items():
        result = subprocess.run([entry['path'], '--version'], capture_output=True, text=True, timeout=10)
        require(result.returncode == 0, 'selected CLI version probe failed')
        versions[name] = result.stdout.strip()
    source_version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    require(versions == {'codex': 'codex-cli 0.164.0', 'opencode2': 'opencode v2.0.32', 'fr': 'fr ' + source_version},
            'installed CLI releases differ from the reviewed local setup: ' + json.dumps(versions))
    checker_paths = {'python': runtime['python'],
        **{name: runtime['rust_root'] / 'bin' / name for name in ('rustc', 'cargo', 'rustdoc')}}
    if milestone in {'M1', 'M3'}:
        checker_paths.update({name: runtime['lean_root'] / 'bin' / name for name in ('lean', 'lake', 'leanc')})
    else:
        checker_paths.update({name: runtime[name] for name in ('node', 'typescript')})
        checker_paths['objcopy'] = runtime['rust_root'] / 'lib/rustlib/aarch64-apple-darwin/bin/rust-objcopy'
    checkers = {name: {'path': str(path.absolute()), 'sha256': hash_file(path)} for name, path in checker_paths.items()}
    checker_versions = {}
    names = ['python', 'rustc', 'cargo'] + (['lean', 'lake'] if milestone in {'M1', 'M3'} else ['node', 'typescript', 'objcopy'])
    for name in names:
        argv = [checkers[name]['path'], '--version']
        if name == 'typescript':
            argv.insert(0, checkers['node']['path'])
        result = subprocess.run(argv, capture_output=True, text=True, timeout=10)
        require(result.returncode == 0, 'local checker version probe failed: ' + name)
        checker_versions[name] = result.stdout.strip()
    require(checker_versions['rustc'].startswith('rustc 1.98.1 '), 'choose the inspected local Rust 1.98.1 toolchain')
    if milestone in {'M1', 'M3'}:
        require('4.28.0' in checker_versions['lean'], 'choose the inspected Lean 4.28.0 toolchain')
    else:
        settings = json.loads((ROOT / CASE_DIRECTORIES[milestone] / 'checkers.json').read_text())
        require(source_version == settings['fr'] and checker_versions['node'] == 'v' + settings['node'] and
                checker_versions['typescript'] == 'Version ' + settings['typescript']['version'] and
                checker_versions['python'] == 'Python ' + settings['local_python'], 'M2 checkers differ from their pins')
    if milestone == 'M3':
        python_packages = {}
        settings = json.loads((ROOT / CASE_DIRECTORIES[milestone] / 'checkers.json').read_text())
        require(checker_versions['python'] == 'Python ' + settings['local_python'], 'M3 Python differs from its pin')
        prefix = subprocess.run([checkers['lean']['path'], '--print-prefix'],
                                capture_output=True, text=True, timeout=10)
        require(prefix.returncode == 0 and Path(prefix.stdout.strip()).resolve() == runtime['lean_root'].resolve(),
                'the Lean binary must belong to the explicitly selected installation')
        source_root = runtime['lean_root'].resolve() / 'src/lean'
        require((source_root / 'Init/Data/Int').is_dir(), 'the selected Lean installation needs its standard library sources')
        checkers['lean']['source_root'] = str(source_root)
    else:
        packages = subprocess.run([checkers['python']['path'], '-c',
            'import importlib.metadata,json;print(json.dumps({name:importlib.metadata.version(name) '
            'for name in ["pytest","pluggy","packaging","iniconfig","pygments"]}))'],
            capture_output=True, text=True, timeout=10)
        require(packages.returncode == 0, 'the study Python needs its exact five pytest wheels')
        python_packages = json.loads(packages.stdout)
        require(python_packages == {'pytest': '9.1.1', 'pluggy': '1.7.0', 'packaging': '26.3',
                                    'iniconfig': '2.3.1', 'pygments': '2.21.0'}, 'local checker wheels differ')
    seeds = {}
    for task in proposed['requirements']['tasks']:
        directory = inputs / task
        receipt = json.loads((directory / 'seed.json').read_text())
        require(receipt['task'] == task and hash_file(directory / 'original.tar.gz') == receipt['archive_sha256'],
                'hosted task seed differs')
        seeds[task] = {'archive': str((directory / 'original.tar.gz').resolve()), 'receipt': receipt}
    packets = {row['id']: prompt(row['task'], row['setup'], checkers, milestone) for row in proposed['attempts']}
    value = {'source': identity(), 'schedule': proposed, 'binaries': paths, 'components': components, 'versions': versions,
             'seeds': seeds, 'prompts': packets, 'implementation': implementation_bindings(milestone),
             'checkers': checkers, 'checker_versions': checker_versions, 'python_packages': python_packages,
             'limits': LIMITS, 'client_finish_seconds': CLIENT_FINISH_SECONDS,
             'public_check_environment': dict(PUBLIC_CHECK_ENVIRONMENT),
             'outer_guard': '/Users/zardoz/.codex/tools/fr-local-guard.py',
             'execution': 'User-requested local Codex and OpenCode2 CLIs with their existing account stores. No credential transfer.',
             'accounting': {'actual_cost': None, 'estimated_cost': None,
                'provider_request_cap_enforced': False, 'token_cap_enforced': False,
                'explanation': 'Native CLI output is recorded. These CLIs do not expose a shared hard token/request cap. '
                               'Enforced limits are wall time, sampled memory/CPU/disk/output and the outer workstation guard.'}}
    if milestone == 'M2':
        value['cargo_home'] = str(runtime['cargo_home'].resolve())
        require((runtime['cargo_home'] / 'registry/src').is_dir(), 'prepare the actual pinned offline dependencies first')
        source_paths = list((runtime['cargo_home'] / 'registry/src').rglob('*'))
        source_paths += list(runtime['typescript'].resolve().parent.parent.rglob('*'))
        source_paths.append(checker_paths['objcopy'].parent.parent / 'lib/libLLVM.dylib')
        value['runtime_source_bindings'] = {str(path.resolve()): hash_file(path) for path in sorted(source_paths)
                                          if path.is_file()}
        require(value['runtime_source_bindings'], 'freeze actual offline compiler and dependency inputs')
        value['fr_build'] = verified_fr_build(value['source'], paths['fr']['sha256'], runtime['fr_build'])
        receipts = json.loads(runtime['controls'].read_text())
        require(set(receipts) == {'controls', 'shared_setup'} and
                set(receipts['controls']) == {'acceptance', 'delivery', 'native', 'private'},
                'freeze every successful control and shared preparation before the first call')
        for name, receipt in receipts['controls'].items():
            require(receipt['passed'] is True and receipt['live_model_calls'] == 0 and
                    str(receipt['run']).isdigit() and len(receipt['source_commit']) == 40 and
                    receipt['bindings'] and all(hash_file(ROOT / path) == digest
                                               for path, digest in receipt['bindings'].items()),
                    'control source changed after its successful GitHub run: ' + name)
            require(all(hashlib.sha256(tasks.comparison_grade.BASE.git(ROOT, 'show',
                    receipt['source_commit'] + ':' + path).stdout).hexdigest() == digest
                    for path, digest in receipt['bindings'].items()), 'control source commit differs: ' + name)
        private = receipts['controls']['private']
        require(private['checked_controls'] == 20 and private['image'].startswith('sha256:') and
                all(len(private[name]) == 64 for name in ('cache_archive_sha256', 'registry_archive_sha256')),
                'freeze all private rejection controls, the immutable image and compiler archives')
        value.update(controls=receipts['controls'], shared_setup=receipts['shared_setup'])
    elif milestone == 'M3':
        from .proof_writing_tasks import validate_controls
        value['fr_build'] = verified_fr_build(value['source'], paths['fr']['sha256'], runtime['fr_build'])
        receipt = validate_controls(json.loads(runtime['controls'].read_text()))
        require(receipt['bindings'] == implementation_bindings('M3'), 'M3 control receipt must bind every checker and task input')
        require(all(hash_file(ROOT / path) == digest for path, digest in receipt['bindings'].items()),
                'proof controls changed after their successful hosted checks')
        require(all(hashlib.sha256(tasks.comparison_grade.BASE.git(ROOT, 'show',
                    receipt['source_commit'] + ':' + path).stdout).hexdigest() == digest
                    for path, digest in receipt['bindings'].items()), 'proof control source commit differs')
        value['controls'] = receipt
        value['formal_inputs'] = {}
        for task in seeds:
            prepared = json.loads((inputs / task / 'project.json').read_text())
            require(prepared['original'] == seeds[task]['receipt']['original'], 'fixed proof input differs from its seed')
            value['formal_inputs'][task] = {name: prepared[name] for name in
                ('source', 'spec', 'obligation', 'goal_digest', 'assumptions', 'arithmetic')}
            value['formal_inputs'][task].update(
                model_sha256=hashlib.sha256(prepared['model'].encode()).hexdigest(),
                property_sha256=hashlib.sha256(encoded(prepared['property'])).hexdigest())
    value['sha256'] = hashlib.sha256(encoded(value)).hexdigest()
    tasks.save(output / 'plan.json', value)
    return value


def usage(client, events):
    if client == 'codex':
        completed = [row['usage'] for row in events if row.get('type') == 'turn.completed']
        return {'reported': completed, 'complete_turns': len(completed), 'actual_cost': None, 'exported': None,
                'model_observed': False, 'explanation': 'The CLI turn totals do not report observed model identity or billed charges.'}
    steps = [row['part'] for row in events if row.get('type') == 'step_finish']
    return {'reported': [{'tokens': row['tokens'], 'catalog_cost': row['cost']} for row in steps],
            'complete_steps': len(steps), 'actual_cost': None, 'exported': None,
            'explanation': 'A zero catalog cost is not an observed account charge.'}


def record_opencode_accounting(report, binary, session, project, env, output, profile, events, deadline):
    remaining = deadline - elapsed_time()
    if remaining <= 0:
        report['status'] = 'failed'
        report['session_accounting'] = {'status': 'deadline-exceeded', 'exported': False}
        return
    try:
        saved = export_session(binary, session, project, env, output, timeout_seconds=min(10, remaining))
    except subprocess.TimeoutExpired:
        report['status'] = 'failed'
        report['session_accounting'] = {'status': 'timed-out', 'exported': False}
        return
    observed = verify_export(saved, profile, session, events)
    tasks.save(output / 'session-export.json', saved)
    report['usage']['exported'] = observed
    report['session_accounting'] = {'status': 'completed', 'exported': True}
    if observed['outcome'] != 'succeeded':
        report['status'] = 'failed'


def retain_result(project, output, report):
    """Keep bounded failed source too, and always remove per-attempt compiler output."""
    try:
        report['final_source'] = tasks.sources(project)
        require(sum((project / name).stat().st_size for name in report['final_source']) <= 32 * 1024**2,
                'delivered source exceeds its 32 MiB archive limit')
        patch = project / 'change.patch'
        if patch.is_file() and not patch.is_symlink():
            if patch.stat().st_size > 1024**2:
                report['collection_errors'].append({'stage': 'patch', 'error': 'Delivered patch exceeds 1 MiB.'})
            else:
                shutil.copyfile(patch, output / 'change.patch')
                report['patch_sha256'] = hash_file(output / 'change.patch')
                report['patch_delivered'] = True
        with tarfile.open(output / 'candidate.tar.gz', 'w:gz') as archive:
            for name in sorted(report['final_source']):
                archive.add(project / name, arcname=name, recursive=False)
        report['candidate_archive_sha256'] = hash_file(output / 'candidate.tar.gz')
    except Exception as error:
        report['collection_errors'].append({'stage': 'source capture', 'error': type(error).__name__ + ': ' + str(error)})
    finally:
        try:
            target = project / 'target'
            if target.is_symlink():
                target.unlink()
            elif target.is_dir():
                shutil.rmtree(target)
        except Exception as error:
            report['collection_errors'].append({'stage': 'compiler cleanup', 'error': type(error).__name__ + ': ' + str(error)})
        if report['collection_errors']:
            report['status'] = 'failed'


def attempt(plan_path, attempt_id):
    entered, entered_at, awake_started = elapsed_time(), time.time(), time.monotonic()
    require(sys.platform == 'darwin', 'run the requested local CLI attempts on macOS')
    value = json.loads(plan_path.read_text())
    milestone = value['schedule']['milestone']
    unsigned = {key: item for key, item in value.items() if key != 'sha256'}
    require(hashlib.sha256(encoded(unsigned)).hexdigest() == value['sha256'], 'local execution plan changed')
    checkout = identity()
    require(value['source']['native_inputs'] == checkout['native_inputs'] and value['schedule'] == prepare(ROOT, milestone),
            'committed native source or study requirements changed')
    require(value['implementation'] == implementation_bindings(milestone), 'study code, skills or checking inputs changed')
    require(value['limits'] == LIMITS, 'local limits changed')
    require(value['client_finish_seconds'] == CLIENT_FINISH_SECONDS and
            value['public_check_environment'] == PUBLIC_CHECK_ENVIRONMENT,
            'public checking settings changed after freezing')
    for binary in [*value['binaries'].values(), *value['components'].values(), *value['checkers'].values()]:
        require(hash_file(Path(binary['path'])) == binary['sha256'], 'a selected native binary changed')
    rows = [row for row in value['schedule']['attempts'] if row['id'] == attempt_id]
    require(len(rows) == 1, 'attempt is not in the frozen milestone schedule')
    row = rows[0]
    output = plan_path.parent / 'attempts' / attempt_id
    output.mkdir(parents=True, exist_ok=False)
    report = {'id': attempt_id, 'status': 'started', 'plan_sha256': value['sha256'], 'profile': row['profile'],
              'task': row['task'], 'setup': row['setup'], 'independent_outcome': None,
              'controller_checkout': checkout,
              'actual_cost': None, 'started_at': entered_at, 'verification_seconds': elapsed_time() - entered,
              'timing_clock': 'includes-system-sleep', 'finished_at': None, 'suspended_seconds': None,
              'process': None, 'usage': None, 'session_accounting': None, 'error': None, 'collection_errors': [],
              'checking_input_changes': [],
              'patch_delivered': False, 'patch_sha256': None, 'complete_attempt_deadline_missed': False,
              'scope': 'Local native CLI attempt; independently grade on GitHub.'}
    tasks.save(output / 'result.json', report)
    started = entered
    project = None
    checking_inputs = {}
    try:
        project = Path('/private/tmp') / ('fr-' + milestone.lower() + '-' + value['sha256'][:12]) / attempt_id / 'project'
        seed = value['seeds'][row['task']]
        unpack(Path(seed['archive']), seed['receipt'], project)
        tasks.comparison_grade.BASE.initialize(project)
        (project / '.git/info/exclude').write_text('\n'.join(sorted((tasks.IGNORED - {'.git'}) | {'change.patch'})) + '\n')
        binary_directory = project.parent / 'bin'
        binary_directory.mkdir()
        if row['setup'] == 'fr':
            (binary_directory / 'fr').symlink_to(value['binaries']['fr']['path'])
            shutil.copytree(ROOT / 'skills/fr', project / '.agents/skills/fr')
        env = {key: os.environ[key] for key in ('HOME', 'PATH', 'LANG', 'CODEX_HOME', 'XDG_CONFIG_HOME',
            'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME', 'SSL_CERT_FILE', 'CODEX_CA_CERTIFICATE') if key in os.environ}
        checkers = value['checkers']
        tool_path = [binary_directory, Path(checkers['python']['path']).parent, Path(checkers['rustc']['path']).parent]
        tool_path.append(Path(checkers['lean' if milestone in {'M1', 'M3'} else 'node']['path']).parent)
        env.update(PATH=os.pathsep.join(map(str, tool_path)) + os.pathsep + env['PATH'], PYTHONDONTWRITEBYTECODE='1',
            FUN_REFACTOR_CACHE=str(project / '.fr/cache'),
            CARGO_NET_OFFLINE='true', RUSTUP_TOOLCHAIN='1.98.1', FR_LEAN_JOBS='1', LEAN_NUM_THREADS='1', RAYON_NUM_THREADS='1',
            RUSTC=checkers['rustc']['path'], RUSTDOC=checkers['rustdoc']['path'], PYTEST_DISABLE_PLUGIN_AUTOLOAD='1',
            GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1',
            RUST_TEST_THREADS='1', OPENCODE_DISABLE_AUTOUPDATE='1', OPENCODE_DISABLE_LSP_DOWNLOAD='1',
            OPENCODE_DISABLE_DEFAULT_PLUGINS='1', OPENCODE_DISABLE_MODELS_FETCH='1', DISABLE_TELEMETRY='1',
            GOMAXPROCS='1', BUN_JSC_useJIT='false')
        env.update(value['public_check_environment'])
        if milestone == 'M2':
            require(all(hash_file(Path(path)) == expected for path, expected in value['runtime_source_bindings'].items()),
                    'offline dependency or TypeScript checker source changed')
            env['CARGO_HOME'] = value['cargo_home']
            directory = project / '.fr'
            directory.mkdir(exist_ok=True)
            if row['task'] == 'regex-escape-len':
                from . import project_edits_public as checker
                checker.install(project)
                checking_inputs[checker.CHECKER] = hash_file(project / checker.CHECKER)
            tool_deadline = entered_at + LIMITS['wall_seconds'] - value['client_finish_seconds']
            declared = [{**check, 'cwd': '.', 'timeout_seconds': 120, 'covers': ['public M2 project checks']}
                        for check in m2_checks(row['task'], {name: tool['path'] for name, tool in checkers.items()}, tool_deadline)]
            tasks.save(directory / 'checks.json', {'schema': 1, 'checks': declared})
            checking_inputs['.fr/checks.json'] = hash_file(directory / 'checks.json')
            report['provided_checking_inputs'] = checking_inputs
        elif milestone == 'M3':
            directory = project / '.fr'
            directory.mkdir(exist_ok=True)
            declared = [{'name': 'lean', 'argv': [checkers['lake']['path'], '-d', 'specs', 'build', '--wfail'],
                         'cwd': '.', 'timeout_seconds': 120, 'covers': ['fixed Lean theorem without proof gaps']}]
            tasks.save(directory / 'checks.json', {'schema': 1, 'checks': declared})
            checking_inputs['.fr/checks.json'] = hash_file(directory / 'checks.json')
            report['provided_checking_inputs'] = checking_inputs
        require((shutil.which('fr', path=env['PATH']) is not None) == (row['setup'] == 'fr'), 'wrong fr availability')
        profile = value['schedule']['profiles'][row['profile']]
        if profile['client'] == 'opencode2':
            references = [checkers['lean']['source_root']] if milestone == 'M3' else []
            config = configuration(profile, row['setup'], project, read_only_directories=references)
            env['OPENCODE_CONFIG_CONTENT'] = json.dumps(config)
            tasks.save(output / 'requested-config.json', config)
        prompt_text = execution_prompt(value['prompts'][attempt_id], started_at=entered_at,
                                       wall_seconds=LIMITS['wall_seconds'], finish_seconds=value['client_finish_seconds'])
        (output / 'prompt.txt').write_text(prompt_text)
        argv, stdin = invocation(profile, value['binaries'], project, output, prompt_text, 'workspace-write')
        tasks.save(output / 'command.json', argv)
        report['project'] = str(project)
        tasks.save(output / 'result.json', report)
        with (output / 'native.stdout').open('xb') as stdout, (output / 'native.stderr').open('xb') as stderr:
            remaining = LIMITS['wall_seconds'] - (elapsed_time() - started)
            require(remaining > 0, 'attempt deadline expired during preparation')
            process = bounded_run(argv, stdin, stdout, stderr, project, env=env, cwd=project,
                                  **{**LIMITS, 'wall_seconds': remaining})
        report['process'] = process
        events = []
        for line in (output / 'native.stdout').read_bytes().splitlines():
            try:
                events.append(json.loads(line))
            except ValueError:
                report['incomplete_json_lines'] = report.get('incomplete_json_lines', 0) + 1
        report['usage'] = usage(profile['client'], events)
        report['status'] = 'completed' if process['exit_code'] == 0 and not process['stop_reason'] else 'failed'
        if profile['client'] == 'opencode2':
            if not events:
                report['status'] = 'failed'
                report['session_accounting'] = {'status': 'no-native-events', 'exported': False}
            else:
                session = argv[argv.index('--session') + 1]
                require(all(event['sessionID'] == session for event in events), 'native stream belongs to another session')
                record_opencode_accounting(report, value['binaries']['opencode2']['path'], session,
                    project, env, output, profile, events, started + LIMITS['wall_seconds'])
        elif profile['client'] == 'codex' and report['usage']['complete_turns'] != 1:
            report['status'] = 'failed'
        report['fr_contribution'] = None
        return report
    except Exception as error:
        report.update(status='failed', error=type(error).__name__ + ': ' + str(error))
        raise
    finally:
        if project is not None and project.is_dir():
            report['checking_input_changes'] = checking_input_changes(project, checking_inputs)
            if report['checking_input_changes']:
                report['status'] = 'failed'
            retain_result(project, output, report)
        report['elapsed_seconds'] = elapsed_time() - started
        report['finished_at'] = time.time()
        report['suspended_seconds'] = max(0, report['elapsed_seconds'] - (time.monotonic() - awake_started))
        report['complete_attempt_deadline_missed'] = report['elapsed_seconds'] > LIMITS['wall_seconds']
        if report['complete_attempt_deadline_missed']:
            report.update(status='failed', complete_attempt_deadline_missed=True)
        tasks.save(output / 'result.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    freezing = commands.add_parser('freeze')
    freezing.add_argument('--inputs', type=Path, required=True)
    freezing.add_argument('--milestone', choices=('M1', 'M2', 'M3'), required=True)
    freezing.add_argument('--fr', type=Path, required=True)
    freezing.add_argument('--codex', type=Path, required=True)
    freezing.add_argument('--opencode2', type=Path, required=True)
    freezing.add_argument('--runtime', type=Path, required=True)
    freezing.add_argument('--output', type=Path, required=True)
    running = commands.add_parser('attempt')
    running.add_argument('--plan', type=Path, required=True)
    running.add_argument('--id', required=True)
    args = parser.parse_args()
    if args.operation == 'freeze':
        result = freeze(args.inputs.resolve(), {name: getattr(args, name).resolve() for name in ('fr', 'codex', 'opencode2')},
                        {name: Path(path).absolute() for name, path in json.loads(args.runtime.read_text()).items()},
                        args.output.resolve(), args.milestone)
        print(json.dumps({'plan_sha256': result['sha256'], 'attempts': len(result['schedule']['attempts'])}))
    else:
        result = attempt(args.plan.resolve(), args.id)
        print(json.dumps({key: result[key] for key in ('id', 'status', 'elapsed_seconds', 'independent_outcome')}))
