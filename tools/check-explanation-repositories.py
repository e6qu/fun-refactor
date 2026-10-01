#!/usr/bin/env python3
"""Runner-only behavior controls for the pinned explanation rubrics; no model calls."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from agent_eval.rehearsal_evidence import source_bundle
from agent_eval.workspace_bundle import unpack

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "tests/agent-eval/opencode"

CONTROLS = {
    "cache-expiration": r'''
from cachetools import Cache, TTLCache
clock = [0]
cache = TTLCache(4, 5, timer=lambda: clock[0])
cache['a'] = 'value'
clock[0] = 5
assert 'a' not in cache
try:
    cache['a']
except KeyError:
    pass
else:
    raise AssertionError('expired entry remained readable')
assert Cache.__len__(cache) == 1
assert list(cache.expire()) == [('a', 'value')]
assert Cache.__len__(cache) == 0
clock[0] = 10
cache['b'] = 'first'
clock[0] = 14
cache['b'] = 'replacement'
clock[0] = 15
assert cache['b'] == 'replacement'
clock[0] = 19
assert 'b' not in cache
print('deadline, physical removal, refresh and expired pairs passed')
''',
    "key-rotation": r'''
from itsdangerous import Serializer, Signer
from itsdangerous.signer import SigningAlgorithm
keys = [b'old-test-key', b'new-test-key']
assert Signer(keys).sign(b'value') == Signer(keys[-1]).sign(b'value')
seen = []
class Observe(SigningAlgorithm):
    def verify_signature(self, key, value, sig):
        seen.append(key)
        return False
assert not Signer(keys, key_derivation='none', algorithm=Observe()).verify_signature(b'value', b'c2ln')
assert seen == list(reversed(keys))
fallbacks = []
class Fallback(Signer):
    def __init__(self, secret_key, **kwargs):
        fallbacks.append(secret_key)
        super().__init__(secret_key, **kwargs)
signers = list(Serializer(keys, fallback_signers=[Fallback]).iter_unsigners())
assert type(signers[0]) is Signer and signers[0].secret_keys == keys
assert fallbacks == keys
assert all(isinstance(signer, Fallback) for signer in signers[1:])
print('signing key, verification order and fallback order passed')
''',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=BASE / "explanations.json")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    for task in manifest["tasks"]:
        with tempfile.TemporaryDirectory(prefix="fr-explanation-control-") as tmp:
            workspace = Path(tmp) / "source"
            unpack(source_bundle(task, args.manifest.parent), workspace, 1024**2)
            # Only disposable CI runs execute the reviewed upstream Python source.
            code = "import sys; sys.dont_write_bytecode = True; sys.path.insert(0, sys.argv[1]);\n"
            code += CONTROLS[task["id"]]
            package = workspace / "src" if (workspace / "src").is_dir() else workspace
            subprocess.run([sys.executable, "-I", "-B", "-c", code, str(package)], check=True, timeout=30)
            print(json.dumps({"task": task["id"], "repository": task["repository"],
                              "revision": task["revision"], "passed": True}))


if __name__ == "__main__":
    main()
