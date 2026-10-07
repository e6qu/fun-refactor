import io, json, sys
from pathlib import Path
sys.path.insert(0, '/Users/zardoz/projects/fun-refactor/tools')
from agent_eval import bounded_host, terminal_review_runner as runner
root = Path('/private/tmp/fr-provider-local-20261007')
folder = root / 'model-list'
folder.mkdir(exist_ok=False)
models = json.loads(Path('tests/agent-eval/opencode/reviews/2026-10-06-packaging-inputs/design.json').read_bytes())['models']
env = runner.configured_environment(folder, models[0], folder / 'unused.json')
settings = json.loads(env['OPENCODE_CONFIG_CONTENT'])
settings['enabled_providers'] = [m['providerID'] for m in models]
settings['provider'] = {m['providerID']: {'models': {m['modelID']: {'name': m['modelID'], 'limit': {'context': m['context'], 'output': m['output']}}}} for m in models}
settings['mcp'] = {}
env.update(OPENCODE_CONFIG_CONTENT=json.dumps(settings), OPENCODE_MODELS_PATH=str(root / 'catalog.json'), BUN_OPTIONS='', RAYON_NUM_THREADS='1')
out, err = io.BytesIO(), io.BytesIO()
process = bounded_host.run(['/Users/zardoz/.opencode/bin/opencode', '--pure', 'models', '--verbose'], b'', out, err, folder,
    env=env, cwd=folder, wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768*1024**2,
    disk_bytes=16*1024**2, transcript_bytes=1024**2)
import importlib.util
spec = importlib.util.spec_from_file_location('catalog_check', Path('tools/check-client-catalog.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
listed = probe.models(out.getvalue())
public = json.loads((root / 'catalog.json').read_bytes())
resolved = {}
for model in models:
    key = model['providerID'] + '/' + model['modelID']
    api = listed[key]['api']
    expected = {'id': model['modelID'], 'url': public[model['providerID']]['api'], 'npm': public[model['providerID']]['npm']}
    resolved[key] = {'api_matches_public_catalog': api == expected, 'context': listed[key]['limit']['context'], 'output': listed[key]['limit']['output']}
    assert api == expected
result = {'process': process, 'models': {m['providerID']+'/'+m['modelID']: m['providerID']+'/'+m['modelID'] in listed for m in models},
          'resolved': resolved, 'stdout_bytes': len(out.getvalue()), 'stderr_bytes': len(err.getvalue()), 'live_compatibility_verified': False}
(folder / 'result.json').write_text(json.dumps(result, sort_keys=True))
print(json.dumps(result, sort_keys=True))
