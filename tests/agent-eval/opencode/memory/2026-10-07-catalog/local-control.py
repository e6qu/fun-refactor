import hashlib, json, os, platform, subprocess, sys
from pathlib import Path
repo = Path('/Users/zardoz/projects/fun-refactor')
root = Path('/private/tmp/fr-catalog-local-20261007')
root.mkdir(exist_ok=False)
client = Path('/Users/zardoz/.opencode/bin/opencode')
with client.open('rb') as stream:
    identity = hashlib.file_digest(stream, 'sha256').hexdigest()
assert platform.machine() == 'arm64'
assert identity == '7b63b34fafabded7d9231f6a9032755d0cdeaf8b9d2b70df8e25535471469eea'
catalog = root / 'catalog.json'
catalog.write_bytes(b'{}\n')
command = [sys.executable, '-B', str(repo / 'tools/check-terminal-reviews.py'), 'check',
           str(root / 'control'), '--case', 'configured', '--fr',
           str(repo / 'target/agent-tools/fr-v0.53.0/fr'), '--opencode', str(client)]
(root / 'invocation.json').write_text(json.dumps({'command': command,
    'opencode_sha256': identity, 'catalog_sha256': hashlib.sha256(catalog.read_bytes()).hexdigest(),
    'environment_overrides': {'OPENCODE_MODELS_PATH': str(catalog), 'BUN_OPTIONS': '', 'RAYON_NUM_THREADS': '1'}}, sort_keys=True))
environment = dict(os.environ, OPENCODE_MODELS_PATH=str(catalog), BUN_OPTIONS='', RAYON_NUM_THREADS='1')
result = subprocess.run(command, env=environment)
if result.returncode == 0:
    process = json.loads((root / 'control/configured/process.json').read_bytes())
    print(json.dumps(process, sort_keys=True))
    assert process['sampled_aggregate_rss_bytes'] <= 640 * 1024**2
sys.exit(result.returncode)
