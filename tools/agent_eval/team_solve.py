"""Coordinate fresh Luna, GLM and Kimi tasks through files and checked patches."""
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

from . import selected_local as local, selected_tasks as common
from .bounded_host import elapsed_time, run as bounded_run
from .selected_plan import ROOT, encoded, require

ROLES = ('plan', 'ideate-a', 'ideate-b', 'solve', 'review', 'repair')
WORKFLOWS = {
    'team': ('plan', 'ideate-a', 'ideate-b', 'solve', 'initial-check', 'review', 'repair', 'final-check'),
    'single': ('solve', 'final-check'),
}
OUTPUTS = {'plan': 'plan.md', 'ideate-a': 'proposal-a.md', 'ideate-b': 'proposal-b.md',
           'solve': 'decision.md', 'review': 'review.md', 'repair': 'repair.md'}
INPUTS = {'plan': (), 'ideate-a': ('plan.md',), 'ideate-b': ('plan.md',),
          'solve': ('plan.md', 'proposal-a.md', 'proposal-b.md'),
          'review': ('plan.md', 'proposal-a.md', 'proposal-b.md', 'decision.md', 'initial-check.json'),
          'repair': ('plan.md', 'proposal-a.md', 'proposal-b.md', 'decision.md', 'initial-check.json', 'review.md')}
BRIEF = {
    'plan': 'Explain the requested result, break down the work, and identify what the checks must establish. Do not edit code.',
    'ideate-a': 'Propose a direct approach, with a correctness argument and concrete edge cases. Do not edit code.',
    'ideate-b': 'Propose a different approach and explain its tradeoffs and edge cases. Do not edit code.',
    'solve': 'Read both proposals, choose an approach, and implement the requested change. Explain your choice.',
    'review': 'Read the actual initial check result and logs. Explain any observed failure and a precise repair. Do not edit code. A passing check needs no repair; a suspicion cannot overturn its verdict.',
    'repair': 'Repair the observed failure using the review and actual check logs. Explain the change. There is no further repair round.',
}
DATA = Path('.fr/team-solve')
GUARD = Path('/Users/zardoz/.codex/tools/fr-local-guard.py')
SETTINGS_KEYS = {'project', 'task', 'output', 'profiles', 'binaries', 'fr_build', 'roles',
                 'editable_files', 'editable_regions', 'checks', 'reference_directories',
                 'require_change', 'limits', 'protected_files', 'check_environment', 'workflow'}


def load(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 1024**2, 'read a bounded regular JSON file')
    return json.loads(path.read_text())


def relative(name):
    require(isinstance(name, str) and name and not Path(name).is_absolute() and
            all(part not in common.IGNORED | {'.', '..', '', 'change.patch'} for part in name.split('/')),
            'use an explicit project-relative source path without reserved directories')
    return name


def sources(project):
    result = {};total = 0
    for directory, folders, files in os.walk(project, followlinks=False):
        folders[:] = [f for f in folders if f not in common.IGNORED]
        require(all(not (Path(directory) / f).is_symlink() for f in folders), 'source directories must not be symlinks')
        for name in files:
            if name == 'change.patch':
                continue
            path = Path(directory) / name
            require(path.is_file() and not path.is_symlink(), 'source must be a regular file')
            size = path.stat().st_size;total += size
            require(size <= 1024**2 and total <= 32 * 1024**2 and len(result) < 4096,
                    'use at most 4096 files, 32 MiB of source and 1 MiB per source file')
            result[str(path.relative_to(project))] = {'sha256': local.hash_file(path), 'mode': path.stat().st_mode & 0o777}
    return result


def settings(path):
    value = load(path)
    require(set(value) == SETTINGS_KEYS, 'supply every explicit workflow setting; unknown settings are refused')
    for key in ('project', 'task', 'output', 'profiles', 'fr_build'):
        require(isinstance(value[key], str) and Path(value[key]).is_absolute(), key + ' needs an absolute path')
    require(value['workflow'] in WORKFLOWS, 'choose team or single explicitly')
    required_roles = set(WORKFLOWS[value['workflow']]) & set(ROLES)
    require(set(value['roles']) == required_roles, 'assign exactly the roles in the selected workflow')
    profiles = load(Path(value['profiles']))
    expected = load(ROOT / 'tests/agent-eval/proof-writing/profiles.json')
    require(profiles == expected and set(value['roles'].values()) <= set(profiles), 'assign only the pinned profiles without model substitution')
    require(set(value['binaries']) == {'fr', 'codex', 'opencode2'} and
            all(isinstance(p, str) and Path(p).is_absolute() for p in value['binaries'].values()), 'supply exactly the three absolute executable paths')
    require(type(value['require_change']) is bool, 'declare whether delivery requires a source change')
    require(set(value['limits']) == {'prepare_seconds', 'role_seconds', 'check_seconds', 'run_seconds', 'parallel_roles'},
            'declare preparation, role, check and total budgets and serial scheduling')
    caps = {'prepare_seconds': 60, 'role_seconds': 150, 'check_seconds': 120, 'run_seconds': 1200, 'parallel_roles': 1}
    require(all(type(value['limits'][k]) is int and 0 < value['limits'][k] <= cap for k, cap in caps.items()),
            'team limits exceed the declared caps; existing solo deadlines are unchanged')
    require(value['limits']['role_seconds'] > local.CLIENT_FINISH_SECONDS, 'reserve client finishing time within the role deadline')
    require(isinstance(value['editable_files'], list) and isinstance(value['editable_regions'], list), 'declare editable files and regions')
    files = [relative(p) for p in value['editable_files']]
    regions = value['editable_regions']
    for region in regions:
        require(set(region) == {'path', 'begin', 'end'} and all(isinstance(region[k], str) and region[k] for k in region), 'declare each region path and its two markers')
        relative(region['path'])
        require(region['begin'] != region['end'] and '\n' not in region['begin'] + region['end'], 'region markers must be distinct single-line text')
    require(files or regions, 'declare the requested change boundary')
    names = files + [r['path'] for r in regions]
    require(len(names) == len(set(names)) <= 128, 'each edited path needs one explicit file or region rule')
    require(isinstance(value['protected_files'], list) and value['protected_files'] and
            len(set(value['protected_files'])) == len(value['protected_files']), 'list the trusted test or proof inputs explicitly')
    require(not set(map(relative, value['protected_files'])) & set(names), 'trusted checking inputs cannot be editable files')
    require(isinstance(value['check_environment'], dict) and all(isinstance(k, str) and isinstance(v, str)
            for k, v in value['check_environment'].items()), 'declare the additional checker environment explicitly, including an empty object')
    require(isinstance(value['checks'], list) and 0 < len(value['checks']) <= 8, 'supply one to eight deterministic checks')
    for check in value['checks']:
        require(set(check) == {'name', 'argv', 'cwd', 'timeout_seconds', 'covers', 'verdict'}, 'declare every checker field')
        require(isinstance(check['argv'], list) and check['argv'] and all(isinstance(a, str) and a for a in check['argv']) and
                Path(check['argv'][0]).is_absolute(), 'checkers need explicit argv and an absolute executable')
        require(check['cwd'] == '.' and type(check['timeout_seconds']) is int and
                0 < check['timeout_seconds'] <= value['limits']['check_seconds'], 'checks run at the project root within the check budget')
        require(isinstance(check['name'], str) and check['name'] and isinstance(check['covers'], list) and check['covers'] and
                all(isinstance(c, str) and c for c in check['covers']), 'name each check and what it establishes')
        require(check['verdict'] in {'exit-code', 'json-passed'}, 'choose exit-code or json-passed explicitly')
    require(len({c['name'] for c in value['checks']}) == len(value['checks']), 'check names must be unique')
    require(isinstance(value['reference_directories'], list) and all(isinstance(p, str) and Path(p).is_absolute() and
            Path(p).is_dir() and Path(p) == Path(p).resolve() and not any(c in p for c in '*?')
            for p in value['reference_directories']), 'declare existing canonical absolute reference directories without wildcards')
    return value, profiles


def fixed_region_hash(text, rule):
    begin, end = rule['begin'], rule['end']
    require(text.count(begin) == text.count(end) == 1 and text.splitlines().count(begin) == text.splitlines().count(end) == 1,
            'preserve the one original marker pair on complete lines')
    start = text.index('\n', text.index(begin)) + 1
    finish = text.rfind('\n', 0, text.index(end)) + 1
    require(start <= finish and text.index(begin) < text.index(end), 'keep ordered complete marker lines')
    return hashlib.sha256(encoded([text[:start], text[finish:]])).hexdigest()


def boundary(plan, project):
    current, original = sources(project), plan['source']
    changed = {p for p in current.keys() | original.keys() if current.get(p) != original.get(p)}
    rules = {r['path']: r for r in plan['settings']['editable_regions']}
    require(changed <= set(plan['settings']['editable_files']) | set(rules), 'a role changed source outside the declared boundary')
    for path in changed & rules.keys():
        require(path in current and current[path]['mode'] == original[path]['mode'], 'keep the marked file and its mode')
        require(fixed_region_hash((project / path).read_text(), rules[path]) == plan['regions'][path],
                'keep the claim, markers and all source outside the editable region unchanged')
    return current


def copy_source(origin, destination):
    destination.mkdir(parents=True, exist_ok=False)
    for name in sources(origin):
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(origin / name, path)


def initialize(project):
    common.comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    common.comparison_grade.BASE.git(project, 'add', '-f', '--', *sources(project))
    common.comparison_grade.BASE.git(project, 'commit', '-qm', 'Snapshot the supplied project for team work')
    (project / '.git/info/exclude').write_text('\n'.join(sorted(common.IGNORED | {'change.patch'})) + '\n')


def check_settings(plan):
    return {'schema': 1, 'checks': [{k: v for k, v in c.items() if k != 'verdict'} for c in plan['settings']['checks']]}


def prepare(value, profiles, run):
    started = elapsed_time()
    project = Path(value['project'])
    require(project.is_dir() and project == project.resolve(), 'select a canonical project directory')
    require(not run.is_relative_to(project) and not project.is_relative_to(run), 'keep run records outside the supplied project')
    git = common.comparison_grade.BASE.git
    require(Path(git(project, 'rev-parse', '--show-toplevel').stdout.decode().strip()) == project and
            not git(project, 'status', '--porcelain').stdout.strip(), 'start from a clean committed project root')
    task = Path(value['task'])
    require(task.is_file() and not task.is_symlink() and 0 < task.stat().st_size <= 32768, 'supply a nonempty task file of at most 32 KiB')
    seed = run / 'seed';seed.mkdir()
    tracked = git(project, 'ls-files', '-z').stdout.decode().split('\0')
    count = total = 0
    for name in filter(None, tracked):
        if set(Path(name).parts) & common.IGNORED:
            continue
        source = project / name
        require(source.is_file() and not source.is_symlink() and source.stat().st_size <= 1024**2, 'snapshot only bounded regular tracked source files')
        count += 1;total += source.stat().st_size
        require(count <= 4096 and total <= 32 * 1024**2, 'the source snapshot exceeds its file or byte limit')
        destination = seed / name;destination.parent.mkdir(parents=True, exist_ok=True);shutil.copy2(source, destination)
    source = sources(seed)
    require(source, 'the supplied project has no source files')
    require(set(value['protected_files']) <= source.keys(), 'trusted inputs must exist in the tracked source snapshot')
    regions = {r['path']: fixed_region_hash((seed / r['path']).read_text(), r) for r in value['editable_regions']}
    binaries, companions = local.freeze_binaries({k: Path(v) for k, v in value['binaries'].items()}, run, {'codex', 'opencode2'})
    versions = {}
    for name in ('codex', 'opencode2', 'fr'):
        remaining = value['limits']['prepare_seconds'] - (elapsed_time() - started)
        require(remaining > 0, 'preparation deadline expired')
        result = subprocess.run([binaries[name]['path'], '--version'], capture_output=True, text=True, timeout=min(10, remaining))
        require(result.returncode == 0 and len(result.stdout) <= 4096, 'native version probe failed')
        versions[name] = result.stdout.strip()
    clients = load(ROOT / 'tests/agent-eval/selected-agents/clients.json')
    require(versions['codex'] == 'codex-cli ' + clients['codex']['version'] and versions['opencode2'] == 'opencode v' + clients['opencode2']['version'],
            'use the pinned native releases; no replacement client is selected: ' + json.dumps(versions))
    identity = local.identity()
    built = local.verified_fr_build(identity, binaries['fr']['sha256'], Path(value['fr_build']))
    (run / 'task.md').write_bytes(task.read_bytes())
    shutil.copytree(ROOT / 'skills/fr', run / 'skill')
    checker_hashes = {c['argv'][0]: local.hash_file(Path(c['argv'][0])) for c in value['checks']}
    plan = {'settings': value, 'profiles': profiles, 'source': source, 'regions': regions, 'binaries': binaries,
            'companions': companions, 'versions': versions, 'fr_build': built, 'checker_hashes': checker_hashes,
            'task_sha256': local.hash_file(run / 'task.md'), 'skill': tree_hashes(run / 'skill'), 'implementation': {str(p.relative_to(ROOT)): local.hash_file(p) for p in
                (ROOT / 'tools/agent_eval/team_solve.py', ROOT / 'tools/agent_eval/team_solve_native.py')},
            'created_at': time.time(), 'upstream': 'https://github.com/guregodevo/memdoor/tree/597c2cf641aec10e7c4cbf0f28be5236ef3cc91d/examples/workflows/team-solve'}
    plan['sha256'] = hashlib.sha256(encoded(plan)).hexdigest()
    require(len(encoded(plan)) <= 1024**2, 'the frozen plan exceeds its 1 MiB record limit')
    common.save(run / 'plan.json', plan)
    copy_source(seed, run / 'candidate');initialize(run / 'candidate')
    (run / 'artifacts').mkdir();(run / 'records').mkdir();(run / 'work').mkdir()
    require(elapsed_time() - started <= value['limits']['prepare_seconds'], 'preparation exceeded its deadline')


def verified_plan(run, expected):
    plan = load(run / 'plan.json')
    require(plan['sha256'] == expected == hashlib.sha256(encoded({k: v for k, v in plan.items() if k != 'sha256'})).hexdigest(), 'the frozen team plan changed')
    require(local.hash_file(run / 'task.md') == plan['task_sha256'] and sources(run / 'seed') == plan['source'], 'the task or original source changed')
    require(tree_hashes(run / 'skill') == plan['skill'], 'the supplied fr skill changed')
    for p, digest in plan['implementation'].items():
        require(local.hash_file(ROOT / p) == digest, 'team implementation changed during a run')
    for b in [*plan['binaries'].values(), *plan['companions'].values()]:
        require(local.hash_file(Path(b['path'])) == b['sha256'], 'a frozen native executable changed')
    return plan


def tree_hashes(directory):
    result = {}
    for path in directory.rglob('*'):
        require(not path.is_symlink(), 'workflow artifacts must not be symlinks')
        if path.is_file():
            result[str(path.relative_to(directory))] = local.hash_file(path)
    return result


def retain_draft(project, record, result):
    """Keep failed work privately too; collection failure cannot make a run pass."""
    try:
        current = sources(project)
        with tarfile.open(record / 'candidate.tar.gz', 'w:gz') as archive:
            for name in sorted(current):
                archive.add(project / name, arcname=name, recursive=False)
        outbox = project / DATA / 'out'
        if outbox.is_dir():
            handoffs = tree_hashes(outbox)
            require(len(handoffs) <= 8 and all((outbox / p).stat().st_size <= 8192 for p in handoffs), 'failed handoffs exceed the collection limit')
            shutil.copytree(outbox, record / 'handoff')
        result['candidate_archive_sha256'] = local.hash_file(record / 'candidate.tar.gz')
    except Exception as error:
        result.update(status='failed', collection_error=type(error).__name__ + ': ' + str(error))


def stage(plan, run, role, seconds, agent):
    started = elapsed_time();record = run / 'records' / role;record.mkdir(exist_ok=False)
    result = {'role': role, 'profile': plan['settings']['roles'][role], 'status': 'started', 'elapsed_seconds': None}
    common.save(record / 'result.json', result)
    project = run / 'work' / role
    try:
        before = sources(run / 'candidate')
        copy_source(run / 'candidate', project);initialize(project)
        root = project / DATA;inbox = root / 'in';outbox = root / 'out'
        inbox.mkdir(parents=True);outbox.mkdir()
        shutil.copyfile(run / 'task.md', inbox / 'task.md')
        inputs = INPUTS[role] if plan['settings']['workflow'] == 'team' else ()
        for name in inputs:
            shutil.copyfile(run / 'artifacts' / name, inbox / name)
        if role in {'review', 'repair'}:
            shutil.copytree(run / 'records' / 'initial-check' / 'logs', inbox / 'check-logs')
        shutil.copytree(run / 'skill', project / '.agents/skills/fr')
        common.save(project / '.fr/checks.json', check_settings(plan))
        immutable = {'inbox': tree_hashes(inbox), 'skill': tree_hashes(project / '.agents'),
                     'checks': local.hash_file(project / '.fr/checks.json'),
                     'head': common.comparison_grade.BASE.git(project, 'rev-parse', 'HEAD').stdout}
        brief = BRIEF[role] if plan['settings']['workflow'] == 'team' else (
            'Implement the requested change using the task, copied project and declared checks. Explain the change. '
            'This is one direct agent session; no plan, proposals, review or repair session is supplied.')
        prompt = (brief + '\nRead .fr/team-solve/in/task.md and the supplied files in that input folder. '
                  'Your working directory is the copied project root. Source and test paths in the task are relative to this root. '
                  'The .fr/team-solve/in folder contains handoff notes and check logs, not copies of source or test files. '
                  'This is a fresh context: use files, not prior conversation. Read .agents/skills/fr/SKILL.md and use fr where it helps. '
                  'Write your concise handoff to .fr/team-solve/out/' + OUTPUTS[role] + '. '
                  'Only the solver and repair worker may edit the declared source paths or marked regions. '
                  'The exact change boundary is: ' + json.dumps({'files': plan['settings']['editable_files'], 'regions': plan['settings']['editable_regions']}) + '. '
                  'Keep the task, claims, tests, checker settings, input files and skill unchanged. '
                  'Do not delegate, call other models, browse the Internet, commit, push, or inspect accounts or host settings. '
                  'Run tools serially. Do not build fr or run repository-wide gates. The outer resource guard is already active; do not start another. '
                  'Read complete bounded JSON or select decoded fields; do not cut JSON with head or tail. '
                  'The coordinator runs the declared checks and captures the final patch. A successful fr process can still report passed=false. '
                  'Finish your handoff within this role deadline.')
        remaining = seconds - (elapsed_time() - started)
        require(remaining > local.CLIENT_FINISH_SECONDS, 'role preparation consumed its deadline')
        result['native'] = agent(plan, project, record, role, prompt, remaining)
        require(result['native']['status'] == 'completed', 'native role failed; preserve it and do not retry')
        require(tree_hashes(inbox) == immutable['inbox'] and tree_hashes(project / '.agents') == immutable['skill'] and
                local.hash_file(project / '.fr/checks.json') == immutable['checks'] and
                common.comparison_grade.BASE.git(project, 'rev-parse', 'HEAD').stdout == immutable['head'], 'a role changed protected workflow inputs')
        after = boundary(plan, project)
        require(sources(run / 'candidate') == before, 'a role changed the coordinator candidate outside its workspace')
        if role not in {'solve', 'repair'}:
            require(after == before, 'a planning or review role edited source')
        require(set(tree_hashes(outbox)) == {OUTPUTS[role]}, 'produce exactly the declared handoff file')
        handoff = outbox / OUTPUTS[role]
        require(0 < handoff.stat().st_size <= 8192, 'handoffs must contain 1 to 8192 bytes')
        require(elapsed_time() - started <= seconds, 'complete role deadline missed')
        shutil.copyfile(handoff, run / 'artifacts' / OUTPUTS[role])
        if role in {'solve', 'repair'}:
            for name in after.keys() - before.keys():
                common.comparison_grade.BASE.git(project, 'add', '-N', '-f', '--', name)
            patch = common.comparison_grade.BASE.git(project, 'diff', '--binary', 'HEAD').stdout
            require(len(patch) <= 1024**2, 'role patch exceeds 1 MiB')
            (record / 'change.patch').write_bytes(patch)
            candidate = run / 'candidate'
            for name in before.keys() - after.keys():
                (candidate / name).unlink()
            for name in after:
                target = candidate / name;target.parent.mkdir(parents=True, exist_ok=True);shutil.copy2(project / name, target)
        result['status'] = 'completed'
    except Exception as error:
        result.update(status='failed', error=type(error).__name__ + ': ' + str(error))
        raise
    finally:
        if project.exists():
            retain_draft(project, record, result)
        result['elapsed_seconds'] = elapsed_time() - started
        if result['elapsed_seconds'] > seconds:
            result.update(status='failed', error='complete role deadline missed')
        common.save(record / 'result.json', result)
        if project.exists():
            shutil.rmtree(project)


def check(plan, run, name, seconds):
    record = run / 'records' / name;record.mkdir(exist_ok=False);logs = record / 'logs';logs.mkdir()
    common.save(record / 'result.json', {'status': 'started', 'passed': False})
    try:
        return check_candidate(plan, run, name, seconds, record, logs)
    except Exception as error:
        common.save(record / 'result.json', {'status': 'failed', 'passed': False, 'error': type(error).__name__ + ': ' + str(error)})
        raise


def check_candidate(plan, run, name, seconds, record, logs):
    project = run / 'candidate';before = boundary(plan, project);started = elapsed_time();rows = []
    (project / '.fr').mkdir(exist_ok=True);common.save(project / '.fr/checks.json', check_settings(plan))
    fixed = local.hash_file(project / '.fr/checks.json')
    for index, item in enumerate(plan['settings']['checks']):
        require(local.hash_file(Path(item['argv'][0])) == plan['checker_hashes'][item['argv'][0]], 'checker executable changed')
        remaining = min(item['timeout_seconds'], seconds - (elapsed_time() - started))
        require(remaining > 0, 'check deadline expired')
        out = logs / f'{index}.stdout';err = logs / f'{index}.stderr'
        with out.open('wb') as stdout, err.open('wb') as stderr:
            process = bounded_run(item['argv'], b'', stdout, stderr, project, wall_seconds=remaining,
                                  transcript_bytes=1024**2, rss_bytes=local.LIMITS['rss_bytes'],
                                  disk_bytes=local.LIMITS['disk_bytes'], cpu_limit_seconds=local.LIMITS['cpu_limit_seconds'],
                                  env={**os.environ, **plan['settings']['check_environment']}, cwd=project)
        common.save(logs / f'{index}.process.json', process)
        require(process['stop_reason'] is None, 'checker hit a resource or time limit; stop and inspect the cause')
        passed = process['exit_code'] == 0
        if item['verdict'] == 'json-passed':
            try:
                payload = json.loads(out.read_text())
                passed = passed and isinstance(payload, dict) and payload.get('passed') is True
            except ValueError:
                passed = False
        rows.append({'name': item['name'], 'passed': passed, 'process': process,
                     'stdout': f'check-logs/{index}.stdout', 'stderr': f'check-logs/{index}.stderr'})
    require(sources(project) == before and local.hash_file(project / '.fr/checks.json') == fixed, 'a checker rewrote source or settings')
    changed = before != plan['source']
    result = {'status': 'completed', 'passed': all(row['passed'] for row in rows) and (changed or not plan['settings']['require_change']),
              'source_changed': changed, 'checks': rows, 'elapsed_seconds': elapsed_time() - started,
              'scope': 'Only the declared checks and change boundary; no general correctness claim.'}
    require(result['elapsed_seconds'] <= seconds, 'complete checker deadline missed')
    common.save(record / 'result.json', result);common.save(run / 'artifacts' / (name + '.json'), result)
    for directory in ('target', '.lake', 'specs/.lake'):
        cache = project / directory
        if cache.is_dir() and not cache.is_symlink():
            shutil.rmtree(cache)
    return result


def guarded_worker(run, operation, expected, name, seconds):
    log = run / (name + '-guard.log')
    command = [str(GUARD), sys.executable, '-B', '-m', 'agent_eval.team_solve', operation,
               '--run', str(run), '--expected', expected, '--name', name, '--seconds', str(seconds)]
    env = dict(os.environ, PYTHONPATH=str(ROOT / 'tools') + os.pathsep + str(ROOT / 'sdk/python/src'))
    with log.open('x') as stream:
        process = subprocess.run([sys.executable, *command], cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
    require(process.returncode == 0, 'guarded workload failed; inspect ' + str(log) + '; never retry a started stage')


def execute(run, plan, started, worker):
    result = {'status': 'started', 'passed': False, 'plan_sha256': plan['sha256'], 'roles': {},
              'actual_cost': None, 'elapsed_seconds': None, 'workflow': plan['settings']['workflow'],
              'comparison_scope': 'Separate workflow comparison; no earlier M3 attempt is reclassified.'}
    common.save(run / 'result.json', result)
    try:
        for name in WORKFLOWS[plan['settings']['workflow']]:
            remaining = plan['settings']['limits']['run_seconds'] - (elapsed_time() - started)
            require(remaining > 0, 'the complete run deadline expired')
            if name == 'repair' and load(run / 'artifacts/initial-check.json')['passed']:
                result['roles'][name] = {'status': 'skipped', 'reason': 'initial checks passed; preserve this candidate'}
                common.save(run / 'result.json', result)
                continue
            operation = '_check' if name.endswith('-check') else '_stage'
            limit = plan['settings']['limits']['check_seconds' if operation == '_check' else 'role_seconds']
            worker(run, operation, plan['sha256'], name, min(limit, remaining))
            verified_plan(run, plan['sha256'])
            result['roles'][name] = load(run / 'records' / name / 'result.json')
            common.save(run / 'result.json', result)
            require(result['roles'][name]['status'] == 'completed', 'a workload failed; inspect its retained record')
        result['passed'] = load(run / 'artifacts/final-check.json')['passed']
        result['status'] = 'completed' if result['passed'] else 'failed'
        for name in sources(run / 'candidate').keys() - plan['source'].keys():
            common.comparison_grade.BASE.git(run / 'candidate', 'add', '-N', '-f', '--', name)
        patch = common.comparison_grade.BASE.git(run / 'candidate', 'diff', '--binary', 'HEAD').stdout
        require(len(patch) <= 1024**2, 'final patch exceeds 1 MiB')
        (run / 'change.patch').write_bytes(patch)
        result['patch_sha256'] = hashlib.sha256(patch).hexdigest()
    except Exception as error:
        result.update(status='failed', passed=False, error=type(error).__name__ + ': ' + str(error))
        raise
    finally:
        for name in (*ROLES, 'initial-check', 'final-check'):
            path = run / 'records' / name / 'result.json'
            if path.is_file() and name not in result['roles']:
                result['roles'][name] = load(path)
        result['elapsed_seconds'] = elapsed_time() - started
        if result['elapsed_seconds'] > plan['settings']['limits']['run_seconds']:
            result.update(status='failed', passed=False, error='complete run deadline missed')
        common.save(run / 'result.json', result)
        common.save(run / 'summary.json', public_summary(result))
    return result


def public_summary(result):
    """No source snapshots, paths, prompts, notes, transcripts or account data."""
    rows = {}
    for role, row in result['roles'].items():
        rows[role] = {k: row[k] for k in ('status', 'profile', 'passed', 'elapsed_seconds') if k in row}
        if 'checks' in row:
            rows[role]['checks'] = [{'name': c['name'], 'passed': c['passed']} for c in row['checks']]
        if 'native' in row:
            rows[role]['process'] = {k: row['native']['process'][k] for k in ('exit_code', 'stop_reason')}
            rows[role]['usage'] = row['native']['usage']
    return {k: result[k] for k in ('status', 'passed', 'plan_sha256', 'actual_cost', 'elapsed_seconds', 'workflow', 'comparison_scope')} | {
        'roles': rows, 'patch_sha256': result.get('patch_sha256'), 'provider_request_count': None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    running = commands.add_parser('run');running.add_argument('--settings', type=Path, required=True)
    preparing = commands.add_parser('_prepare');preparing.add_argument('--settings', type=Path, required=True)
    for operation in ('_stage', '_check'):
        worker = commands.add_parser(operation)
        worker.add_argument('--run', type=Path, required=True);worker.add_argument('--expected', required=True)
        worker.add_argument('--name', required=True);worker.add_argument('--seconds', type=float, required=True)
    args = parser.parse_args()
    if args.operation in {'run', '_prepare'}:
        value, profiles = settings(args.settings.resolve());run = Path(value['output'])
        if args.operation == '_prepare':
            prepare(value, profiles, run);return 0
        require(sys.platform == 'darwin' and GUARD.is_file(), 'local team workloads require the macOS workstation guard')
        project = Path(value['project']).resolve()
        require(run == run.resolve() and not run.is_relative_to(project) and not project.is_relative_to(run), 'use a canonical output directory outside the project')
        started = elapsed_time();run.mkdir(parents=True, exist_ok=False)
        env = dict(os.environ, PYTHONPATH=str(ROOT / 'tools') + os.pathsep + str(ROOT / 'sdk/python/src'))
        with (run / 'prepare-guard.log').open('x') as log:
            process = subprocess.run([sys.executable, str(GUARD), sys.executable, '-B', '-m', 'agent_eval.team_solve', '_prepare',
                                      '--settings', str(args.settings.resolve())], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        require(process.returncode == 0, 'preparation failed; keep the run directory and inspect prepare-guard.log')
        plan = load(run / 'plan.json');result = execute(run, plan, started, guarded_worker)
        print(json.dumps({'status': result['status'], 'passed': result['passed'], 'elapsed_seconds': result['elapsed_seconds'], 'patch': str(run / 'change.patch')}))
        return int(not result['passed'])
    plan = verified_plan(args.run, args.expected)
    require(args.seconds > 0, 'workload deadline must be positive')
    if args.operation == '_stage':
        require(args.name in plan['settings']['roles'], 'select a role declared in this workflow')
        from .team_solve_native import agent
        stage(plan, args.run, args.name, args.seconds, agent)
    else:
        require(args.name in set(WORKFLOWS[plan['settings']['workflow']]) & {'initial-check', 'final-check'}, 'select a declared check phase')
        check(plan, args.run, args.name, args.seconds)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
