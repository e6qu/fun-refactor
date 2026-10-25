#!/usr/bin/env python3
"""Inspect the pinned M1 clients on a runner without credentials or inference."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import urllib.request

from agent_eval.opencode2_client import OpenCode2Server, scripted_configuration
from agent_eval.codex_client import CodexServer

ROOT = Path(__file__).resolve().parents[1]
CLIENTS = ROOT / 'tests/agent-eval/selected-agents/clients.json'
ARCHIVE_BYTES = 256 * 1024**2
OUTPUT_BYTES = 1024**2


def install(entry, destination):
    destination.mkdir(parents=True, exist_ok=False)
    archive = destination / 'client.tar.gz'
    with urllib.request.urlopen(entry['url'], timeout=30) as source, archive.open('xb') as output:
        size = 0
        while block := source.read(1024**2):
            size += len(block)
            if size > ARCHIVE_BYTES:
                raise ValueError('client archive exceeds 256 MiB')
            output.write(block)
    identity = entry['digest']
    with archive.open('rb') as source:
        checksum = hashlib.file_digest(source, identity['algorithm'])
    if identity['encoding'] == 'hex':
        actual = checksum.hexdigest()
    elif identity['encoding'] == 'base64':
        actual = base64.b64encode(checksum.digest()).decode()
    else:
        raise ValueError('unknown digest encoding')
    if actual != identity['value']:
        raise ValueError('client archive identity differs')
    with tarfile.open(archive) as source:
        members = source.getmembers()
        (destination / 'archive-members.json').write_text(json.dumps([
            {'name': item.name, 'size': item.size, 'type': item.type.decode(), 'link': item.linkname}
            for item in members], indent=2) + '\n')
        selected = [item for item in members if item.name == entry['member']]
        if len(members) > 64 or len(selected) != 1:
            raise ValueError('client archive members differ')
        if sum(item.size for item in members) != entry['unpacked_bytes']:
            raise ValueError('client unpacked footprint differs from its pinned inventory')
        member, = selected
        if not member.isfile() or not 0 < member.size <= entry['unpacked_bytes']:
            raise ValueError(f'client executable differs: {member.name}, type={member.type!r}, size={member.size}')
        name = entry['executable']
        if Path(name).name != name or name in {'.', '..'}:
            raise ValueError('client executable must be one filename')
        binary = destination / name
        with source.extractfile(member) as data, binary.open('xb') as output:
            while block := data.read(1024**2):
                output.write(block)
    binary.chmod(0o755)
    archive.unlink()
    for name, companion in entry['companions'].items():
        if companion['version'] != entry['version'] or companion['executable'] != name:
            raise ValueError('client companion differs from its release or executable identity')
        installed = install(companion, destination / ('component-' + name))
        shutil.copy2(installed, destination / name)
    return binary


def environment(root):
    home = root / 'home'
    home.mkdir()
    env = {'PATH': os.environ['PATH'], 'HOME': str(home), 'LANG': 'C.UTF-8',
           'CODEX_HOME': str(home / '.codex'), 'CI': 'true',
           'DISABLE_TELEMETRY': '1', 'RAYON_NUM_THREADS': '1', 'UV_THREADPOOL_SIZE': '1'}
    for key, folder in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                        ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state')]:
        path = home / folder
        path.mkdir()
        env[key] = str(path)
    for name in ('AUTOUPDATE', 'MODELS_FETCH', 'DEFAULT_PLUGINS', 'LSP_DOWNLOAD'):
        env['OPENCODE_DISABLE_' + name] = '1'
    return env


def inspect(name, entry, root):
    binary = install(entry, root)
    env = environment(root)
    arguments = [['--version'], ['--help']]
    arguments += ([['app-server', '--help'], ['exec', '--help']] if name == 'codex' else
                  [['serve', '--help'], ['api', '--help'], ['run', '--help'], ['models', '--help']])
    results = []
    for index, args in enumerate(arguments):
        result = subprocess.run([str(binary), *args], cwd=root, env=env, capture_output=True, timeout=10)
        (root / f'{index}.stdout').write_bytes(result.stdout)
        (root / f'{index}.stderr').write_bytes(result.stderr)
        if result.returncode != 0 or len(result.stdout) + len(result.stderr) > OUTPUT_BYTES:
            raise ValueError(f'{name} {args}: invalid metadata response; inspect retained output')
        if index == 0 and re.search(r'(?<![\d.])' + re.escape(entry['version']) + r'(?![\d.])', result.stdout.decode()) is None:
            raise ValueError(f'{name} reports a different release')
        results.append({'arguments': args, 'returncode': result.returncode,
                        'stdout': f'{index}.stdout', 'stderr': f'{index}.stderr'})
    with binary.open('rb') as source:
        executable_sha256 = hashlib.file_digest(source, 'sha256').hexdigest()
    report = {'release': entry, 'binary_sha256': executable_sha256, 'commands': results,
              'credentials_supplied': False, 'inference_requests': 0}
    report['companions'] = {}
    for name, companion in entry['companions'].items():
        with (root / name).open('rb') as source:
            report['companions'][name] = {'release': companion, 'binary_sha256': hashlib.file_digest(source, 'sha256').hexdigest()}
    if name == 'codex':
        home = Path(env['CODEX_HOME'])
        home.mkdir()
        profile = json.loads((CLIENTS.parent / 'profiles.json').read_text())['codex-luna-fast']
        (home / 'config.toml').write_text('\n'.join(key + ' = ' + json.dumps(value) for key, value in {
            'model': profile['model'], 'model_reasoning_effort': profile['reasoning_effort'],
            'service_tier': profile['service_tier'], 'model_provider': profile['provider'],
            'approval_policy': 'never', 'sandbox_mode': 'workspace-write', 'web_search': 'disabled'}.items()) + '\n')
        with CodexServer(binary, root, env, root / 'api', call_seconds=10, transcript_bytes=4 * 1024**2) as server:
            effective = server.request('config/read', {'includeLayers': False})
            (root / 'effective-config.json').write_text(json.dumps(effective, indent=2) + '\n')
            config = effective['config']
            expected = {'model': profile['model'], 'model_reasoning_effort': profile['reasoning_effort'],
                        'service_tier': profile['service_tier'], 'model_provider': profile['provider']}
            if any(config[key] != value for key, value in expected.items()):
                raise ValueError('native Codex model, reasoning or service tier differs from the selected profile')
            catalog = server.request('model/list', {'includeHidden': True})
            (root / 'models.json').write_text(json.dumps(catalog, indent=2) + '\n')
            if catalog['nextCursor'] is not None:
                raise ValueError('native Codex model list is incomplete')
            report['initialized'] = server.initialized
        report['shutdown'] = server.shutdown
        if server.shutdown['forced'] or server.shutdown['exit_code'] != 0:
            raise ValueError('Codex app-server did not shut down cleanly on EOF')
    if name == 'opencode2':
        with OpenCode2Server(binary, root, env, root / 'api', version=entry['version'],
                             startup_seconds=10, call_seconds=10, reply_bytes=4 * 1024**2) as server:
            document = server.request('GET', '/openapi.json', None, 200, 10)
            (root / 'openapi.json').write_text(json.dumps(document, indent=2) + '\n')
            catalog = server.request('GET', '/api/model', None, 200, 10)
            (root / 'models.json').write_text(json.dumps(catalog, indent=2) + '\n')
            report['server'] = server.info
        report['shutdown'] = server.shutdown
        if server.shutdown['forced']:
            raise ValueError('private server required forced shutdown')
        profiles = json.loads((CLIENTS.parent / 'profiles.json').read_text())
        report['profiles'] = {}
        for profile_name, profile in profiles.items():
            if profile['client'] != 'opencode2':
                continue
            profile_root = root / profile_name
            profile_root.mkdir()
            profile_env = environment(profile_root)
            # No inference is requested here. The unused loopback endpoint cannot reach a paid provider.
            requested = scripted_configuration(profile, 'http://127.0.0.1:1')
            profile_env['OPENCODE_CONFIG_CONTENT'] = json.dumps(requested)
            with OpenCode2Server(binary, profile_root, profile_env, profile_root / 'api', version=entry['version'],
                                 startup_seconds=10, call_seconds=10, reply_bytes=4 * 1024**2) as server:
                config = server.request('GET', '/api/config', None, 200, 10)
                (profile_root / 'effective-config.json').write_text(json.dumps(config, indent=2) + '\n')
                inventory = server.wait_agent('build', 10)
                agents = inventory['catalog']
                (profile_root / 'agents.json').write_text(json.dumps(agents, indent=2) + '\n')
                catalog = server.request('GET', '/api/model', None, 200, 10)
                (profile_root / 'models.json').write_text(json.dumps(catalog, indent=2) + '\n')
                model = profile['model']
                selected = [row for row in catalog['data'] if row['providerID'] == model['providerID'] and row['id'] == model['id']]
                if len(selected) != 1 or not any(row['id'] == model['variant'] for row in selected[0]['variants']):
                    raise ValueError('OpenCode2 did not expose the exact selected model and reasoning variant')
                chosen = [row for row in agents['data'] if row['id'] == 'build']
                if len(chosen) != 1:
                    raise ValueError('OpenCode2 did not expose the explicitly selected built-in agent')
                session = server.request('POST', '/api/session', {'agent': 'build', 'model': model,
                    'title': 'M1 native settings control', 'permissions': requested['permissions'],
                    'location': {'directory': str(profile_root)}}, 200, 10)['data']
                (profile_root / 'session.json').write_text(json.dumps(session, indent=2) + '\n')
                if session['agent'] != 'build' or session['model'] != model or session['permissions'] != requested['permissions']:
                    raise ValueError('OpenCode2 session model, reasoning or tool permissions differ from the requested profile')
                server.request('DELETE', '/api/session/' + session['id'], None, 204, 10)
                report['profiles'][profile_name] = {'agent': chosen[0], 'agent_readiness_polls': inventory['polls'],
                    'session': session, 'model': selected[0],
                    'scope': 'Scripted local provider configuration only; no live admission or price claim.'}
            if server.shutdown['forced']:
                raise ValueError('configured OpenCode2 server required forced shutdown')
            report['profiles'][profile_name]['shutdown'] = server.shutdown
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Run client probes on GitHub under the workstation rules.')
    args.output.mkdir(parents=True, exist_ok=False)
    clients = json.loads(CLIENTS.read_text())
    report = {}
    try:
        for name, entry in clients.items():
            report[name] = {'status': 'running', 'release': entry}
            try:
                report[name] = {'status': 'passed', **inspect(name, entry, args.output / name)}
            except Exception as error:
                report[name].update(status='failed', error=type(error).__name__ + ': ' + str(error))
                raise
    finally:
        (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'clients': list(report), 'live_model_calls': 0}))
