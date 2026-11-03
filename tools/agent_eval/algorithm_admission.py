"""Check the complete local algorithm workflow with scripted replies, before paid sessions."""
import argparse
import hashlib
from importlib import metadata
import json
import math
import os
from pathlib import Path
import re
import shlex
import shutil
import sys

from . import bounded_host as host, proof_algorithms as algorithms, proof_delivery as delivery
from . import selected_local as local, selected_provider as provider, selected_tasks as common
from .opencode2_client import scripted_configuration
from .selected_cli import configuration, invocation, export_session, verify_export
from .selected_guarded import GUARD, IndependentSampler
from .selected_plan import ALGORITHM_STUDIES, ALGORITHM_TASKS, DELIVERY_STUDY, ROOT, encoded, prepare, require

CONTROL_LIMITS = {'wall_seconds': 60, 'rss_bytes': 805306368, 'cpu_limit_seconds': 25,
                  'disk_bytes': 67108864, 'transcript_bytes': 131072}
RESERVE_BYTES = 128 * 1024**2
MARKER = 'COMPLETE_ALGORITHM_WORKFLOW'
NATIVE_REQUIREMENTS = ROOT / 'tests/agent-eval/selected-agents/native-requirements.txt'


def server_packages():
    text = NATIVE_REQUIREMENTS.read_text()
    versions = re.findall(r'/websockets-([0-9.]+)-py3-none-any\.whl --hash=sha256:[0-9a-f]{64}', text)
    require(len(versions) == 1 and len(text.splitlines()) == 1, 'use the explicit hash-locked scripted server requirement')
    return {'websockets': versions[0]}


def native_python():
    required = server_packages()
    try:
        packages = {name: metadata.version(name) for name in required}
    except metadata.PackageNotFoundError as error:
        raise ValueError('Selected Python lacks the scripted server package; install ' + str(NATIVE_REQUIREMENTS)) from error
    require(packages == required, 'scripted server packages differ from the hash-locked requirement')
    return {'executable_sha256': common.digest(Path(sys.executable)),
            'requirements_sha256': common.digest(NATIVE_REQUIREMENTS), 'packages': packages}


def validate_receipts(receipts, binaries, archive_sha256, qualified_run, study, checkers):
    plan = prepare(ROOT, study)
    requirements = plan['requirements']['native_admission']
    require(requirements['complete_workflow_required'] is True and requirements['session_accounting_required'] is True
            and requirements['client_context_reserve_bytes'] == RESERVE_BYTES and
            requirements['control_limits'] == CONTROL_LIMITS, 'admission policy differs from the frozen requirements')
    profiles = json.loads((ROOT / 'tests/agent-eval/proof-writing/profiles.json').read_text())
    setups = delivery.SETUPS if study == DELIVERY_STUDY else ('workflow',)
    expected = {(name + '/' + setup if study == DELIVERY_STUDY else name): (name, setup)
                for name in profiles for setup in setups}
    require(set(receipts) == set(expected), 'qualify every profile and required setup before calls')
    features = ('supplied_proofs', 'saved_edit_export', 'independent_patch_replay') if study == DELIVERY_STUDY else ('recursive_loop_rejected',)
    if study == DELIVERY_STUDY:
        require(requirements['conditions'] == list(delivery.SETUPS) and
                all(requirements[name + '_required'] is True for name in features), 'qualify both delivery setups')
    else:
        require(study in ALGORITHM_STUDIES and requirements['recursive_loop_rejection_required'] is True,
                'choose an explicit algorithm admission study')
    implementation = local.implementation_bindings(study)
    compact = {}
    for identity, (name, setup) in expected.items():
        profile, row = profiles[name], receipts[identity]
        require(row['passed'] is True and row['profile'] == name and row['study'] == study and row['setup'] == setup and
                row['live_model_calls'] == 0 and
                row['upstream_forwarding'] is False and row['scripted_requests'] == 2 and row['scripted_errors'] == 0 and
                all(row[field] is True for field in ('complete_workflow', 'session_accounting', *features)),
                'admission must check both proof regions, rejection, package, patch and accounting: ' + name)
        require(row['fr_sha256'] == binaries['fr']['sha256'] and row['archive_sha256'] == archive_sha256 and
                row['guard_sha256'] == common.digest(GUARD) and row['qualified_run'] == qualified_run and
                row['implementation'] == implementation and row['client_context_reserve_bytes'] == RESERVE_BYTES,
                'admission source, executable, task, guard or resource policy changed: ' + name)
        require(row['client_sha256'] == binaries[profile['client']]['sha256'] and
                row['checker_sha256'] == {key: checkers[key]['sha256'] for key in ('lean', 'lake', 'leanc')},
                'admitted native client or Lean toolchain changed')
        require(row['python_runtime'] == {'executable_sha256': checkers['python']['sha256'],
                'requirements_sha256': common.digest(NATIVE_REQUIREMENTS), 'packages': server_packages()},
                'admitted Python or scripted server packages changed')
        resources = row['resources']
        require(resources['limits'] == {'wall_seconds': 60, 'rss_bytes': 805306368, 'cpu_seconds': 25,
                'disk_bytes': 67108864, 'transcript_bytes': 131072} and resources['stop_reason'] is None and
                resources['exit_code'] == 0 and resources['monitor_error'] is None and resources['launch_error'] is None,
                'admission was interrupted or its limits changed: ' + name)
        ceilings = {'elapsed_seconds': 60, 'sampled_aggregate_rss_bytes': 805306368 - RESERVE_BYTES,
                    'sampled_cpu_seconds': 25, 'sampled_disk_growth_bytes': 67108864, 'transcript_bytes': 131072}
        for field, ceiling in ceilings.items():
            value = resources[field]
            require(type(value) in (int, float) and 0 <= value <= ceiling and math.isfinite(value),
                    'admission lacks its declared resource margin: ' + name + '/' + field)
        compact[identity] = {field: row[field] for field in ('profile', 'study', 'setup', 'fr_sha256', 'client_sha256', 'python_runtime',
                         'checker_sha256', 'guard_sha256', 'archive_sha256', 'qualified_run', 'client_context_reserve_bytes')}
        compact[identity].update(passed=True, complete_workflow=True, **{field: True for field in features},
                             session_accounting=True, resources={field: resources[field] for field in ceilings})
    return {'profiles': compact, 'implementation_sha256': hashlib.sha256(encoded(implementation)).hexdigest(),
            'receipts_sha256': hashlib.sha256(encoded(receipts)).hexdigest()}


def script(lean_root, search_path, study, setup):
    if study == DELIVERY_STUDY:
        return delivery_script(lean_root, search_path, setup)
    require(study in ALGORITHM_STUDIES and setup == 'workflow', 'choose the complete algorithm workflow explicitly')
    target = 'specs/FrSpecs/Euclid.lean::'
    lines = ['set -eu', 'export PATH=' + shlex.quote(search_path),
             shlex.join(['fr', '--json', 'spec', 'proof-state', target + 'euclid_correct',
        '--from', '.fr/proof-tactics', '--at', '1:5', '--token-limit', '4096', '--timeout-seconds', '30'])]
    bad = shlex.join(['fr', '--json', 'spec', 'proof-check', target + 'euclid_correct',
                     '--from', '.fr/admission-bad.lean', '--token-limit', '4096'])
    lines += [f'if {bad} > .fr/admission-rejected.json; then task_rejection_status=0; else task_rejection_status=$?; fi',
              shlex.join(['python3', '-c', 'import json,sys; r=json.load(open(".fr/admission-rejected.json")); '
                  'assert int(sys.argv[1]) in (0,1) and r["passed"] is False; '
                  'assert r["receipt"] is None and "Possibly looping simp theorem" in json.dumps(r)']) + ' "$task_rejection_status"']
    for name in algorithms.OBLIGATIONS:
        reference = ROOT / algorithms.DIRECTORY / 'controls' / (name.replace('_', '-') + '.lean')
        for operation, field in [('proof-check', 'passed'), ('prove', 'applied')]:
            arguments = ['fr', '--json', 'spec', operation, target + name, '--from', str(reference)]
            arguments += ['--token-limit', '4096'] if operation == 'proof-check' else ['--write']
            report = '.fr/admission-' + name + '-' + operation + '.json'
            lines += [shlex.join(arguments) + ' > ' + shlex.quote(report),
                      shlex.join(['python3', '-c', 'import json; assert json.load(open(' + repr(report) + '))['
                                  + repr(field) + '] is True'])]
    lines += [shlex.join([str(lean_root / 'bin/lake'), '-d', 'specs', 'build', '--wfail']) +
              ' > .fr/admission-package.log 2>&1',
              'git diff --binary HEAD > change.patch', 'test -s change.patch',
              'printf ' + shlex.quote(MARKER + '\n')]
    return '\n'.join(lines) + '\n'


def delivery_script(lean_root, search_path, setup):
    require(setup in delivery.SETUPS, 'choose the delivery setup explicitly')
    lines = ['set -eu', 'export PATH=' + shlex.quote(search_path), 'cat .agents/skills/fr/SKILL.md > /dev/null']
    if setup == 'finish-card':
        lines.append('cat .fr/finish-card.txt > /dev/null')
    saves = []
    for name in algorithms.OBLIGATIONS:
        for operation, field in (('proof-check', 'passed'), ('prove', 'applied')):
            report = '.fr/admission-' + name + '-' + operation + '.json'
            argv = ['fr', '--json', 'spec', operation, algorithms.SPEC + '::' + name,
                    '--from', '.fr/' + name + '.tactics']
            argv += ['--token-limit', '4096'] if operation == 'proof-check' else ['--write']
            lines += [shlex.join(argv) + ' > ' + shlex.quote(report), shlex.join(['python3', '-c',
                'import json; assert json.load(open(' + repr(report) + '))[' + repr(field) + '] is True'])]
            if operation == 'prove':
                saves.append(report)
    lines.append(shlex.join([str(lean_root / 'bin/lake'), '-d', 'specs', 'build', '--wfail']) +
                 ' > .fr/admission-package.log 2>&1')
    export = ('import json,subprocess; rows=[json.load(open(name)) for name in ' + repr(saves) + ']; '
              'assert all(row["status"]=="written" and row["proof_checked"] is True for row in rows); '
              'subprocess.run(["fr","--json","history","patch",*[str(row["transaction"]) for row in rows],'
              '"--output","change.patch"],check=True)')
    lines += [shlex.join(['python3', '-c', export]), 'test -s change.patch', 'printf ' + shlex.quote(MARKER + '\n')]
    return '\n'.join(lines) + '\n'


def child(args):
    require(args.study in ALGORITHM_TASKS, 'choose the native admission study explicitly')
    validator = delivery.validate_controls if args.study == DELIVERY_STUDY else algorithms.validate_controls
    validator(json.loads(args.controls.read_text()), args.study)
    local.identity()
    profile = json.loads((ROOT / 'tests/agent-eval/proof-writing/profiles.json').read_text())[args.profile]
    project = args.output / 'project'
    seed = json.loads((args.inputs / 'seed.json').read_text())
    local.unpack(args.inputs / 'original.tar.gz', seed, project)
    for fixture, source in [('Euclid.lean', algorithms.SPEC), ('FrSpecs.lean', 'specs/FrSpecs.lean')]:
        require((project / source).read_bytes() == (ROOT / algorithms.DIRECTORY / 'fixtures' / fixture).read_bytes(),
                'admit the exact qualified task, including its entry module')
    common.comparison_grade.BASE.initialize(project)
    directory = project / '.fr'
    directory.mkdir(exist_ok=True)
    protected = {}
    if args.study == DELIVERY_STUDY:
        protected = delivery.install(project, args.setup, args.lean_root / 'bin/lake')
    else:
        require(args.setup == 'workflow', 'original algorithm admission needs the complete workflow')
        shutil.copytree(ROOT / 'skills/fr', project / '.agents/skills/fr')
        (directory / 'proof-tactics').write_text('skip\n')
        (directory / 'admission-bad.lean').write_text('induction b <;> simp [euclid, Nat.gcd]\n')
        checks = {'schema': 1, 'checks': [{'name': 'lean', 'argv': [str(args.lean_root / 'bin/lake'),
            '-d', 'specs', 'build', '--wfail'], 'cwd': '.', 'timeout_seconds': 120,
            'covers': ['fixed Lean theorem without proof gaps']}]}
        common.save(directory / 'checks.json', checks)
    check_digest = common.digest(directory / 'checks.json')
    control_script = args.output / 'complete-workflow.sh'
    tools = args.output / 'bin'
    tools.mkdir()
    (tools / 'fr').symlink_to(args.fr)
    env = {key: os.environ[key] for key in ('HOME', 'PATH', 'LANG', 'CODEX_HOME', 'SSL_CERT_FILE',
           'CODEX_CA_CERTIFICATE', 'PYTHONPATH') if key in os.environ}
    env['PATH'] = os.pathsep.join((str(tools), str(Path(sys.executable).parent),
                                 str(args.lean_root / 'bin'), env['PATH']))
    control_script.write_text(script(args.lean_root, env['PATH'], args.study, args.setup))
    for key, name in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                      ('XDG_STATE_HOME', 'state'), ('XDG_CACHE_HOME', 'cache')]:
        path = args.output / name
        path.mkdir()
        env[key] = str(path)
    env.update(OPENCODE_DISABLE_AUTOUPDATE='1', OPENCODE_DISABLE_LSP_DOWNLOAD='1',
        OPENCODE_DISABLE_DEFAULT_PLUGINS='1', OPENCODE_DISABLE_MODELS_FETCH='1', DISABLE_TELEMETRY='1',
        GOMAXPROCS='1', BUN_JSC_useJIT='false', FR_LEAN_JOBS='1', LEAN_NUM_THREADS='1',
        RAYON_NUM_THREADS='1', PYTHONDONTWRITEBYTECODE='1', OPENAI_API_KEY='sk-scripted-control-not-a-live-key')
    original_chat = provider.chat
    def local_chat(model, command, first):
        require('"' not in str(project) and '\\' not in str(project), 'unsupported scripted directory')
        return original_chat(model, command, first).replace(b'/workspace', str(project).encode())
    provider.chat = local_chat
    transport = provider.websocket_provider if profile['client'] == 'codex' else provider.chat_provider
    command = shlex.join(['/bin/bash', str(control_script)])
    expected_output = [MARKER] if args.study == DELIVERY_STUDY else ['not-checked', 'euclid a b', MARKER]
    with transport(profile, command, expected_output, args.output) as (endpoint, requests, errors, warmups):
        if profile['client'] == 'opencode2':
            config = scripted_configuration(profile, endpoint)
            config.update(configuration(profile, 'fr', project,
                read_only_directories=[(args.lean_root / 'src/lean').resolve()]))
            env['OPENCODE_CONFIG_CONTENT'] = json.dumps(config)
        argv, stdin = invocation(profile, {profile['client']: {'path': str(args.client)}}, project,
            args.output, 'Check the fixed algorithm workflow and report readiness.', 'workspace-write')
        if profile['client'] == 'codex':
            argv += ['--config', 'openai_base_url=' + json.dumps(endpoint + '/v1')]
        with (args.output / 'native.stdout').open('xb') as stdout, (args.output / 'native.stderr').open('xb') as stderr:
            result = host.run(argv, stdin, stdout, stderr, project, env=env, cwd=project,
                              **{**CONTROL_LIMITS, 'wall_seconds': 45})
        require(result['exit_code'] == 0 and result['stop_reason'] is None and not errors and len(requests) == 2
                and provider.COMPLETE.encode() in (args.output / 'native.stdout').read_bytes(),
                'complete native admission failed; inspect private records without raising limits')
        events = [json.loads(line) for line in (args.output / 'native.stdout').read_text().splitlines()]
        if profile['client'] == 'opencode2':
            session = argv[argv.index('--session') + 1]
            saved = export_session(args.client, session, project, env, args.output, timeout_seconds=10)
            require(verify_export(saved, profile, session, events)['outcome'] == 'succeeded', 'native accounting failed')
        else:
            require(profile['client'] == 'codex' and local.usage('codex', events)['complete_turns'] == 1,
                    'Codex admission requires a completed native turn')
        require(not errors and len(requests) == 2, 'accounting caused an extra scripted provider request')
        require(common.digest(directory / 'checks.json') == check_digest, 'admission changed check settings')
        if args.study == DELIVERY_STUDY:
            patch, error = delivery.submission(project, protected)
            require(error is None, 'the native workflow did not preserve its supplied inputs and root patch')
            receiver = args.output / 'receiver'
            common.checkout(project, receiver)
            common.comparison_grade.BASE.git(receiver, 'apply', str(patch))
            require(common.sources(receiver) == common.sources(project), 'the delivered patch omits saved work')
        common.save(args.output / 'child.json', {'passed': True, 'scripted_requests': len(requests),
            'scripted_errors': len(errors), 'complete_workflow': True,
            'recursive_loop_rejected': args.study != DELIVERY_STUDY,
            'supplied_proofs': args.study == DELIVERY_STUDY, 'saved_edit_export': args.study == DELIVERY_STUDY,
            'independent_patch_replay': args.study == DELIVERY_STUDY,
            'session_accounting': True, 'warmups': len(warmups)})


def observe(args):
    require(sys.platform == 'darwin' and args.guard == GUARD, 'use the required local workstation guard')
    require(all(path.is_file() for path in (args.fr, args.client, args.controls, args.lean_root / 'bin/lake',
                args.inputs / 'seed.json', args.inputs / 'original.tar.gz')),
            'admission requires existing pinned tools and qualified task inputs')
    python_runtime = native_python()
    args.output.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.nice(15)
    command = ['python3', str(args.guard), sys.executable, '-B', '-m', 'agent_eval.algorithm_admission', 'guarded']
    for name in ('fr', 'client', 'lean-root', 'inputs', 'controls', 'output', 'guard', 'profile', 'study', 'setup'):
        command += ['--' + name, str(getattr(args, name.replace('-', '_')))]
    sampler, previous = IndependentSampler(args.output), host.sample
    host.sample = sampler
    try:
        with (args.output / 'controller.stdout').open('xb') as stdout, (args.output / 'controller.stderr').open('xb') as stderr:
            resources = host.run(command, b'', stdout, stderr, args.output,
                                 env=dict(os.environ), cwd=ROOT, **CONTROL_LIMITS)
    finally:
        host.sample = previous
    common.save(args.output / 'resources.json', resources)
    common.save(args.output / 'peak.json', sampler.peak)
    detail = json.loads((args.output / 'child.json').read_text()) if (args.output / 'child.json').is_file() else {'passed': False}
    passed = (detail['passed'] and resources['exit_code'] == 0 and resources['stop_reason'] is None
              and resources['monitor_error'] is None and resources['launch_error'] is None
              and resources['sampled_aggregate_rss_bytes'] <= CONTROL_LIMITS['rss_bytes'] - RESERVE_BYTES)
    summary = {**detail, 'passed': passed, 'profile': args.profile, 'live_model_calls': 0,
        'study': args.study, 'setup': args.setup,
        'python_runtime': python_runtime,
        'upstream_forwarding': False, 'fr_sha256': common.digest(args.fr), 'client_sha256': common.digest(args.client),
        'guard_sha256': common.digest(args.guard), 'archive_sha256': common.digest(args.inputs / 'original.tar.gz'),
        'qualified_run': json.loads(args.controls.read_text())['run'],
        'implementation': local.implementation_bindings(args.study),
        'checker_sha256': {name: common.digest(args.lean_root / 'bin' / name) for name in ('lean', 'lake', 'leanc')},
        'client_context_reserve_bytes': RESERVE_BYTES, 'resources': resources}
    common.save(args.output / 'summary.json', summary)
    require(passed, 'admission needs every step and the unchanged memory reserve; inspect private records')
    return {key: summary[key] for key in ('passed', 'profile', 'live_model_calls', 'qualified_run')}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('observe', 'guarded'))
    for name in ('fr', 'client', 'lean-root', 'inputs', 'controls', 'output', 'guard'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--study', choices=tuple(sorted(ALGORITHM_TASKS)), required=True)
    parser.add_argument('--setup', choices=('workflow', *delivery.SETUPS), required=True)
    args = parser.parse_args()
    for name in ('fr', 'client', 'lean_root', 'inputs', 'controls', 'output', 'guard'):
        setattr(args, name, getattr(args, name).resolve())
    if args.mode == 'guarded':
        child(args)
    else:
        print(json.dumps(observe(args)), flush=True)
