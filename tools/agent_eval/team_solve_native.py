"""Run one pinned native client once, inside an already guarded team role."""
import json
import os
import time

from . import selected_local as local, selected_tasks as common
from .bounded_host import elapsed_time, run
from .selected_cli import configuration, invocation
from .selected_plan import require


def environment(plan, project):
    env = {key: os.environ[key] for key in ('HOME', 'PATH', 'LANG', 'CODEX_HOME', 'XDG_CONFIG_HOME',
        'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME', 'SSL_CERT_FILE', 'CODEX_CA_CERTIFICATE') if key in os.environ}
    directory = project / '.fr/bin';directory.mkdir()
    (directory / 'fr').symlink_to(plan['binaries']['fr']['path'])
    env.update(plan['settings']['check_environment'])
    env.update(PATH=str(directory) + os.pathsep + env['PATH'], PYTHONDONTWRITEBYTECODE='1',
        FUN_REFACTOR_CACHE=str(project / '.fr/cache'),
        CARGO_NET_OFFLINE='true', CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0',
        FR_LEAN_JOBS='1', LEAN_NUM_THREADS='1', RAYON_NUM_THREADS='1',
        PYTEST_DISABLE_PLUGIN_AUTOLOAD='1', GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1',
        RUST_TEST_THREADS='1', OPENCODE_DISABLE_AUTOUPDATE='1', OPENCODE_DISABLE_LSP_DOWNLOAD='1',
        OPENCODE_DISABLE_DEFAULT_PLUGINS='1', OPENCODE_DISABLE_MODELS_FETCH='1', DISABLE_TELEMETRY='1',
        GOMAXPROCS='1', BUN_JSC_useJIT='false')
    return env


def agent(plan, project, record, role, prompt, seconds):
    started = elapsed_time()
    profile = plan['profiles'][plan['settings']['roles'][role]]
    report = {'status': 'failed', 'process': {'exit_code': None, 'stop_reason': 'not-started'},
              'usage': {'actual_cost': None}, 'elapsed_seconds': None}
    try:
        env = environment(plan, project)
        if profile['client'] == 'opencode2':
            config = configuration(profile, 'fr', project, read_only_directories=plan['settings']['reference_directories'])
            env['OPENCODE_CONFIG_CONTENT'] = json.dumps(config)
            common.save(record / 'requested-config.json', config)
        remaining = seconds - (elapsed_time() - started)
        require(remaining > local.CLIENT_FINISH_SECONDS, 'native preparation exhausted the role budget')
        prompt = local.execution_prompt(prompt, started_at=time.time(), wall_seconds=remaining,
                                        finish_seconds=local.CLIENT_FINISH_SECONDS)
        (record / 'prompt.txt').write_text(prompt)
        argv, stdin = invocation(profile, plan['binaries'], project, record, prompt, 'workspace-write')
        common.save(record / 'command.json', argv)
        with (record / 'native.stdout').open('xb') as stdout, (record / 'native.stderr').open('xb') as stderr:
            report['process'] = run(argv, stdin, stdout, stderr, project, env=env, cwd=project,
                **{**local.LIMITS, 'wall_seconds': seconds - (elapsed_time() - started)})
        events = []
        for line in (record / 'native.stdout').read_bytes().splitlines():
            try:
                event = json.loads(line)
                require(isinstance(event, dict), 'native event must be an object')
                events.append(event)
            except ValueError:
                report['incomplete_json_lines'] = report.get('incomplete_json_lines', 0) + 1
        report['usage'] = local.usage(profile['client'], events)
        process = report['process']
        report['status'] = 'completed' if process['exit_code'] == 0 and process['stop_reason'] is None else 'failed'
        if profile['client'] == 'codex':
            require(report['usage']['complete_turns'] == 1, 'require exactly one completed native turn')
        else:
            session = argv[argv.index('--session') + 1]
            require(events and all(event['sessionID'] == session for event in events), 'native stream must belong to the fresh session')
            local.record_opencode_accounting(report, plan['binaries']['opencode2']['path'], session,
                project, env, record, profile, events, started + seconds)
    except Exception as error:
        report.update(status='failed', error=type(error).__name__ + ': ' + str(error))
    finally:
        report['elapsed_seconds'] = elapsed_time() - started
        if report['elapsed_seconds'] > seconds:
            report.update(status='failed', error='native accounting exceeded the role deadline')
        common.save(record / 'native-result.json', report)
    return report
