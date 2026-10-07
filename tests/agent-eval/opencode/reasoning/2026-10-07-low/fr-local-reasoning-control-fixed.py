import importlib.util, io, json, sys
from pathlib import Path
repo = Path('/Users/zardoz/projects/fun-refactor')
sys.path.insert(0, str(repo / 'tools'))
from agent_eval import bounded_host, source_reviews, terminal_review_runner as runner
from agent_eval.study import encode
root = Path('/private/tmp/fr-local-reasoning-20261007-fixed')
root.mkdir(exist_ok=False)
script = repo / 'tools/check-review-reasoning.py'
binary = repo / 'target/agent-tools/fr-v0.53.0/fr'
client = Path('/Users/zardoz/.opencode/bin/opencode')
assert source_reviews.identity(client) == '7b63b34fafabded7d9231f6a9032755d0cdeaf8b9d2b70df8e25535471469eea'
command = [sys.executable, '-B', str(script), 'capture', str(root), '--case', '0', '--fr', str(binary), '--opencode', str(client)]
(root / 'invocation.json').write_bytes(encode({'command': command, 'script_sha256': source_reviews.identity(script),
    'helper_sha256': source_reviews.identity(Path(__file__))}))
out, err = io.BytesIO(), io.BytesIO()
process = bounded_host.run(command, b'', out, err, root, wall_seconds=120, cpu_limit_seconds=20,
    rss_bytes=768*1024**2, disk_bytes=16*1024**2, transcript_bytes=1024**2)
(root / 'host.stdout').write_bytes(out.getvalue())
(root / 'host.stderr').write_bytes(err.getvalue())
(root / 'process.json').write_bytes(encode(process))
frozen = json.loads((root / 'plan.json').read_bytes())
snapshots = source_reviews.read_inputs(root)
cell = frozen['plan']['cells'][0]
attempt = root / 'attempts' / cell['id']
runner.cleanup(attempt)
runner.seal(frozen, snapshots, cell, attempt, process)
spec = importlib.util.spec_from_file_location('reasoning_control', script)
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)
result = control.report(root)
(root / 'result.json').write_bytes(encode(result))
print(encode(process).decode())
assert process['sampled_aggregate_rss_bytes'] <= 640*1024**2
