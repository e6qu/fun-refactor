"""Inside a network-free container: selected native apps execute a scripted tool call."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from .codex_client import CodexServer
from .opencode2_client import OpenCode2Server, scripted_configuration
from .selected_plan import ROOT, DIRECTORY, require
from .selected_provider import COMPLETE, chat_provider, websocket_provider

spec = importlib.util.spec_from_file_location('selected_client_probe', ROOT / 'tools/selected-agent-probe.py')
packages = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packages)


def toml(value):
    if isinstance(value, dict):
        return '{' + ', '.join(json.dumps(key) + ' = ' + toml(item) for key, item in value.items()) + '}'
    if isinstance(value, list):
        return '[' + ', '.join(toml(item) for item in value) + ']'
    return json.dumps(value)


def codex(binary, project, env, profile, endpoint, setup, output):
    home = Path(env['CODEX_HOME'])
    home.mkdir()
    config = {'model': profile['model'], 'model_provider': 'openai',
        'openai_base_url': endpoint + '/v1',
        'model_reasoning_effort': profile['reasoning_effort'], 'service_tier': profile['service_tier'],
        'approval_policy': 'never', 'sandbox_mode': 'danger-full-access', 'web_search': 'disabled',
        'features': {'plugins': False, 'multi_agent': False}}
    (home / 'config.toml').write_text('\n'.join(key + ' = ' + toml(value) for key, value in config.items()) + '\n')
    with CodexServer(binary, project, env, output / 'api', call_seconds=10, transcript_bytes=4 * 1024**2) as server:
        login = server.request('account/login/start', {'type': 'apiKey', 'apiKey': 'sk-scripted-control-not-a-live-key'})
        require(login['type'] == 'apiKey', 'Codex did not select the scripted API credential')
        effective = server.request('config/read', {'includeLayers': False})
        (output / 'effective-config.json').write_text(json.dumps(effective, indent=2) + '\n')
        require(all(effective['config'][key] == config[key] for key in (
            'model', 'model_provider', 'model_reasoning_effort', 'service_tier')), 'Codex effective settings changed')
        skills = server.request('skills/list', {'cwds': [str(project)], 'forceReload': True})['data']
        require(len(skills) == 1 and not skills[0]['errors'], 'Codex skill discovery failed')
        require(any(row['name'] == 'fr' for row in skills[0]['skills']) == (setup == 'fr'), 'wrong native Codex fr skill availability')
        thread = server.request('thread/start', {'cwd': str(project), 'model': profile['model'], 'modelProvider': 'openai',
            'approvalPolicy': 'never', 'sandbox': 'danger-full-access', 'ephemeral': True})['thread']['id']
        turn = server.request('turn/start', {'threadId': thread, 'model': profile['model'], 'effort': profile['reasoning_effort'],
            'input': [{'type': 'text', 'text': 'Inspect app.py and report readiness.'}],
            'sandboxPolicy': {'type': 'externalSandbox', 'networkAccess': 'enabled'}})['turn']['id']
        deadline = time.monotonic() + 30
        while True:
            value = server.receive(deadline)
            if value.get('method') == 'turn/completed' and value['params']['turn']['id'] == turn:
                require(value['params']['turn']['status'] == 'completed', 'scripted Codex turn failed')
                break
        require(COMPLETE in json.dumps(server.transcript), 'Codex did not retain the final scripted reply')
    require(server.shutdown == {'exit_code': 0, 'forced': False}, 'Codex required forced shutdown')
    return {'skills': skills, 'shutdown': server.shutdown}


def opencode(binary, project, env, profile, endpoint, setup, output, version):
    config = scripted_configuration(profile, endpoint)
    config['skills'] = [str(project / '.agents/skills')] if setup == 'fr' else []
    env['OPENCODE_CONFIG_CONTENT'] = json.dumps(config)
    with OpenCode2Server(binary, project, env, output / 'api', version=version,
                         startup_seconds=10, call_seconds=10, reply_bytes=4 * 1024**2) as server:
        server.wait_agent('build', 10)
        session = server.request('POST', '/api/session', {'agent': 'build', 'model': profile['model'],
            'permissions': config['permissions'], 'title': 'M1 scripted control',
            'location': {'directory': str(project)}}, 200, 10)['data']
        require(session['model'] == profile['model'], 'OpenCode2 session substituted the model or variant')
        skills = server.request('GET', '/api/skill', None, 200, 10)
        (output / 'skills.json').write_text(json.dumps(skills, indent=2) + '\n')
        require(any(row['name'] == 'fr' for row in skills['data']) == (setup == 'fr'), 'wrong native OpenCode2 fr skill availability')
        selection = profile['model']
        result = subprocess.run([str(binary), 'run', '--server', 'http://127.0.0.1:' + str(server.port),
            '--session', session['id'], '--model', selection['providerID'] + '/' + selection['id'] + '#' + selection['variant'],
            '--agent', 'build', '--format', 'json', '--title', 'M1 scripted control', 'Inspect app.py and report readiness.'],
            cwd=project, env=server.env, capture_output=True, timeout=30)
        (output / 'native.stdout').write_bytes(result.stdout)
        (output / 'native.stderr').write_bytes(result.stderr)
        require(len(result.stdout) + len(result.stderr) <= 1024**2, 'OpenCode2 native output exceeds its byte limit')
        require(result.returncode == 0 and COMPLETE.encode() in result.stdout, 'OpenCode2 scripted turn did not complete')
        exported = server.request('GET', '/api/experimental/session/' + session['id'] + '/export', None, 200, 10)
        (output / 'session-export.json').write_text(json.dumps(exported, indent=2) + '\n')
        server.request('DELETE', '/api/session/' + session['id'], None, 204, 10)
    require(not server.shutdown['forced'], 'OpenCode2 required forced shutdown')
    return {'skills': skills, 'shutdown': server.shutdown}


def run(profile_name, setup):
    require(Path('/.dockerenv').is_file(), 'native exchanges run only inside their network-free container')
    output, project = Path('/diagnostics'), Path('/workspace')
    profiles = json.loads((ROOT / DIRECTORY / 'profiles.json').read_text())
    clients = json.loads((ROOT / DIRECTORY / 'clients.json').read_text())
    profile = profiles[profile_name]
    (project / 'app.py').write_text('def normalize_total(value):\n    return value + 2\n')
    subprocess.run(['git', 'init', '-q', '-b', 'main'], cwd=project, check=True, capture_output=True, timeout=5)
    state = Path('/home/control')
    require(state.is_dir(), 'fresh private client home mount is required')
    env = packages.environment(state)
    env.update(GOMAXPROCS='1', BUN_JSC_useJIT='false', PYTHONDONTWRITEBYTECODE='1')
    if setup == 'fr':
        shutil.copytree('/opt/skill/fr', project / '.agents/skills/fr')
    require((shutil.which('fr', path=env['PATH']) is not None) == (setup == 'fr'), 'wrong fr executable availability')
    command = ('cat .agents/skills/fr/SKILL.md && fr --json project find normalize_total --in app.py --source --bytes 4096'
               if setup == 'fr' else 'python3 -c \'from pathlib import Path; print("app.py"); print(Path("app.py").read_text())\'')
    expected = ['app.py', 'normalize_total', 'return value + 2'] + (['Work with fr'] if setup == 'fr' else [])
    summary = {'profile': profile_name, 'setup': setup, 'passed': False, 'live_model_calls': 0,
               'scope': 'Scripted replies to real native clients, not live outcomes or a cost comparison.'}
    try:
        transport = websocket_provider if profile['client'] == 'codex' else chat_provider
        with transport(profile, command, expected, output) as (endpoint, requests, errors):
            executable = '/opt/client/bin/' + clients[profile['client']]['executable']
            if profile['client'] == 'codex':
                result = codex(executable, project, env, profile, endpoint, setup, output)
            else:
                result = opencode(executable, project, env, profile, endpoint, setup, output,
                                  clients['opencode2']['version'])
            require(len(requests) == 2 and not errors, 'scripted request count or provider validation failed')
        summary.update(passed=True, scripted_requests=len(requests), native=result)
        return summary
    except Exception as error:
        summary['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        (output / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--setup', choices=('files', 'fr'), required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.profile, args.setup)))
